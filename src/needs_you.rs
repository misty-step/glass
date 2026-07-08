//! Needs You (glass-918): the operator's whole input queue as a native Glass
//! view, re-homed from bridge-003 (EPIC: the operator inbox) + bridge-006
//! (answer relay must cover every repo). Parity source is factory-ops's
//! `~/.factory-lanes/scripts/bridge.py` (`render_needs_you`) and
//! `ask-triage.py` (the model curator) -- this module calls the curator as
//! a subprocess and reads its annotation cache, it does not re-judge asks
//! itself (operator ruling 2026-07-05: semantic judgment is a model's job,
//! never a keyword heuristic).
//!
//! Unlike bridge.py's relay (`RELAY + "/bridge-answer"`, an external host
//! bridge-006 flagged as repo-filtered), Glass answers natively: `POST
//! /api/needs-you/answer` calls Powder's `answer_input` directly with no
//! repo filter at all, so every repo's asks are answerable from here.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::extract::Json as AxumJson;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{sanctum_url, shell};

fn powder_base_url() -> Option<String> {
    std::env::var("GLASS_POWDER_API_BASE_URL")
        .ok()
        .filter(|v| !v.is_empty())
}

fn powder_board_url() -> Option<String> {
    let operator_url = std::env::var("GLASS_POWDER_BOARD_URL")
        .ok()
        .filter(|v| !v.is_empty());
    let api_base = powder_base_url();
    powder_board_url_from(operator_url.as_deref(), api_base.as_deref())
}

fn powder_board_url_from(operator_url: Option<&str>, api_base: Option<&str>) -> Option<String> {
    let base = operator_url
        .filter(|value| !value.is_empty())
        .or_else(|| api_base.filter(|value| !value.is_empty()))?;
    Some(board_url_from_base(base))
}

fn board_url_from_base(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    if trimmed.ends_with("/board") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/board")
    }
}

fn powder_api_key() -> Option<String> {
    std::env::var("GLASS_POWDER_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
}

/// The curator's annotation cache path, matching factory-ops's own
/// `.ask-triage.json` convention -- overridable for tests, defaulting to
/// the real path bridge.py and ask-triage.py already share.
fn triage_cache_path() -> std::path::PathBuf {
    std::env::var("GLASS_ASK_TRIAGE_CACHE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
            std::path::PathBuf::from(home)
                .join(".factory-lanes")
                .join(".ask-triage.json")
        })
}

fn triage_script_path() -> std::path::PathBuf {
    std::env::var("GLASS_ASK_TRIAGE_SCRIPT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
            std::path::PathBuf::from(home)
                .join(".factory-lanes")
                .join("scripts")
                .join("ask-triage.py")
        })
}

#[derive(Debug, Deserialize)]
struct AwaitingResponse {
    awaiting: Vec<AwaitingItem>,
    #[serde(default)]
    answered: Vec<AnsweredItem>,
}

#[derive(Debug, Clone, Default)]
struct NeedsYouData {
    awaiting: Vec<AwaitingItem>,
    answered: Vec<AnsweredItem>,
}

#[derive(Debug, Deserialize, Clone)]
struct AwaitingItem {
    card: CardBrief,
    question: Option<QuestionPayload>,
    run: RunInfo,
}

#[derive(Debug, Deserialize, Clone)]
struct CardBrief {
    id: String,
    title: String,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    priority: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct QuestionPayload {
    payload: String,
    #[serde(default)]
    created_at: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
struct RunInfo {
    id: String,
    agent: String,
    #[serde(default)]
    created_at: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
struct AnsweredItem {
    card: CardBrief,
    #[serde(default)]
    question: Option<QuestionPayload>,
    run: RunInfo,
    #[serde(default)]
    answer: Option<String>,
    #[serde(default)]
    answered_at: Option<i64>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct TriageAnnotation {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    ask_line: Option<String>,
    #[serde(default)]
    evidence_links: Vec<String>,
    #[serde(default)]
    situation: Option<String>,
    #[serde(default)]
    options: Vec<String>,
    #[serde(default)]
    recommended_answer: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    judge: Option<String>,
    #[serde(default)]
    run: Option<String>,
}

async fn fetch_awaiting() -> Result<NeedsYouData, String> {
    fetch_awaiting_with_reporting(true).await
}

async fn fetch_awaiting_silent() -> Result<NeedsYouData, String> {
    fetch_awaiting_with_reporting(false).await
}

async fn fetch_awaiting_with_reporting(report_errors: bool) -> Result<NeedsYouData, String> {
    let base = match powder_base_url() {
        Some(base) => base,
        None => {
            if report_errors {
                crate::canary::report_error(
                    "glass.needs_you.fetch.failed",
                    "route=/api/needs-you upstream=powder error_kind=missing_base_url",
                );
            }
            return Err("GLASS_POWDER_API_BASE_URL is not configured".to_string());
        }
    };
    let key = match powder_api_key() {
        Some(key) => key,
        None => {
            if report_errors {
                crate::canary::report_error(
                    "glass.needs_you.fetch.failed",
                    "route=/api/needs-you upstream=powder error_kind=missing_api_key",
                );
            }
            return Err("GLASS_POWDER_API_KEY is not configured".to_string());
        }
    };
    let url = awaiting_input_url_from_api_base(&base);
    let response = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&key)
        .send()
        .await
        .map_err(|err| {
            if report_errors {
                crate::canary::report_error(
                    "glass.needs_you.fetch.failed",
                    "route=/api/needs-you upstream=powder error_kind=transport",
                );
            }
            format!("fetch {url}: {err}")
        })?;
    if !response.status().is_success() {
        if report_errors {
            crate::canary::report_error(
                "glass.needs_you.fetch.failed",
                &format!(
                    "route=/api/needs-you upstream=powder upstream_status={} error_kind=upstream_status",
                    response.status().as_u16()
                ),
            );
        }
        return Err(format!(
            "fetch {url}: upstream returned {}",
            response.status()
        ));
    }
    let mut data = response
        .json::<AwaitingResponse>()
        .await
        .map(|body| NeedsYouData {
            awaiting: body.awaiting,
            answered: body.answered,
        })
        .map_err(|err| {
            if report_errors {
                crate::canary::report_error(
                    "glass.needs_you.fetch.failed",
                    "route=/api/needs-you upstream=powder error_kind=parse",
                );
            }
            format!("parse {url}: {err}")
        })?;
    sort_awaiting(&mut data.awaiting);
    data.answered.sort_by(|a, b| {
        b.answered_at
            .unwrap_or(0)
            .cmp(&a.answered_at.unwrap_or(0))
            .then_with(|| a.card.id.cmp(&b.card.id))
    });
    Ok(data)
}

fn awaiting_input_url_from_api_base(base: &str) -> String {
    format!(
        "{}/api/v1/runs/awaiting-input?limit=100",
        base.trim_end_matches('/')
    )
}

/// factory-ops's own sort contract (bridge.py's `collect_needs_you`):
/// session-repo asks first (conversation beats paperwork), then priority,
/// then card id -- preserved here for parity even though Glass renders
/// grouped by curator kind, not sort order, so the same relative ordering
/// survives within each kind's group.
fn sort_awaiting(items: &mut [AwaitingItem]) {
    const SESSION_REPOS: [&str; 2] = ["factory/session", "session"];
    items.sort_by(|a, b| {
        let a_session = a
            .card
            .repo
            .as_deref()
            .is_some_and(|r| SESSION_REPOS.contains(&r));
        let b_session = b
            .card
            .repo
            .as_deref()
            .is_some_and(|r| SESSION_REPOS.contains(&r));
        b_session
            .cmp(&a_session)
            .then_with(|| {
                let a_priority = a.card.priority.as_deref().unwrap_or("p9");
                let b_priority = b.card.priority.as_deref().unwrap_or("p9");
                a_priority.cmp(b_priority)
            })
            .then_with(|| a.card.id.cmp(&b.card.id))
    });
}

/// Reads whatever the curator has already annotated, keyed by run id --
/// never blocks on a fresh judgment (see `trigger_triage_refresh_once`).
fn load_triage_annotations() -> HashMap<String, TriageAnnotation> {
    let Ok(raw) = std::fs::read_to_string(triage_cache_path()) else {
        return HashMap::new();
    };
    let Ok(cache) = serde_json::from_str::<HashMap<String, TriageAnnotation>>(&raw) else {
        return HashMap::new();
    };
    cache
        .into_values()
        .filter_map(|ann| ann.run.clone().map(|run| (run, ann)))
        .collect()
}

static TRIAGE_RUNNING: AtomicBool = AtomicBool::new(false);

/// Best-effort, fire-and-forget refresh of the curator's annotation cache.
/// Guarded by a single in-flight flag so concurrent requests never spawn
/// duplicate model-calling subprocesses; ask-triage.py's own payload-hash
/// cache makes repeat invocations cheap for already-judged asks. A dead or
/// missing curator script degrades to "untriaged" rows (fail-open,
/// matching bridge.py's own posture) rather than failing the request.
fn trigger_triage_refresh_once() {
    if TRIAGE_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let script = triage_script_path();
    tokio::spawn(async move {
        if script.is_file() {
            let result = tokio::time::timeout(
                Duration::from_secs(90),
                tokio::process::Command::new("python3")
                    .arg(&script)
                    .arg("--quiet")
                    .output(),
            )
            .await;
            match result {
                Ok(Ok(output)) if output.status.success() => {}
                Ok(Ok(output)) => {
                    crate::canary::report_error(
                        "glass.needs_you.triage_refresh.failed",
                        &format!(
                            "task=ask-triage error_kind=exit_status status={}",
                            output.status.code().unwrap_or(-1)
                        ),
                    );
                }
                Ok(Err(_)) => {
                    crate::canary::report_error(
                        "glass.needs_you.triage_refresh.failed",
                        "task=ask-triage error_kind=spawn",
                    );
                }
                Err(_) => {
                    crate::canary::report_error(
                        "glass.needs_you.triage_refresh.failed",
                        "task=ask-triage error_kind=timeout",
                    );
                }
            }
        }
        TRIAGE_RUNNING.store(false, Ordering::SeqCst);
    });
}

fn kind_of(ann: Option<&TriageAnnotation>) -> &'static str {
    match ann.and_then(|a| a.kind.as_deref()) {
        Some("question") => "question",
        Some("act") => "act",
        Some("endorse") => "endorse",
        _ => "decide",
    }
}

fn html_escape(raw: &str) -> String {
    raw.chars()
        .flat_map(|ch| match ch {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect(),
            '>' => "&gt;".chars().collect(),
            '"' => "&quot;".chars().collect(),
            '\'' => "&#39;".chars().collect(),
            _ => vec![ch],
        })
        .collect()
}

fn relative_time(ts: i64, now: DateTime<Utc>) -> String {
    let Some(then) = DateTime::from_timestamp(ts, 0) else {
        return "?".to_string();
    };
    let delta = (now - then).num_seconds();
    if delta < 60 {
        "just now".to_string()
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86_400 {
        format!("{}h ago", delta / 3600)
    } else {
        format!("{}d ago", delta / 86_400)
    }
}

/// One rendered ask row plus its hidden dialog detail markup.
struct Rendered {
    row_and_sheet: String,
}

fn render_item(
    item: &AwaitingItem,
    ann: Option<&TriageAnnotation>,
    now: DateTime<Utc>,
    board_url: &str,
) -> Rendered {
    let card_id = &item.card.id;
    let run_id = &item.run.id;
    let question_text = item
        .question
        .as_ref()
        .map(|q| q.payload.clone())
        .unwrap_or_else(|| item.card.title.clone());
    let kind = kind_of(ann);
    let ask_line = ann.and_then(|a| a.ask_line.clone()).unwrap_or_else(|| {
        question_text
            .lines()
            .next()
            .unwrap_or(&item.card.title)
            .to_string()
    });
    let created_at = item
        .question
        .as_ref()
        .and_then(|q| q.created_at)
        .or(item.run.created_at)
        .unwrap_or_else(|| now.timestamp());
    let age = relative_time(created_at, now);
    let untriaged_marker = if ann.is_none() {
        r#" <span class="ae-tag ae-tag-bare ny-untriaged" title="curator has not judged this ask yet">untriaged</span>"#
    } else {
        ""
    };
    let blocker = ann
        .and_then(|a| a.situation.as_deref())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(item.card.title.as_str());
    let sheet_id = format!("ny-sheet-{}", html_escape(card_id));

    let row = format!(
        r#"<div class="ny-row ny-row-{kind}">
  <span class="ny-row-text">
    <span class="ae-item">{ask_line}</span>{untriaged_marker}<br>
    <span class="ae-dim ny-meta-line">{agent} &middot; powder {card_id} &middot; asked {age} &middot; {blocker}</span>
  </span>
  <button type="button" class="ae-button ae-button-quiet ny-open-btn" data-sheet="{sheet_id}">Answer</button>
</div>"#,
        kind = kind,
        sheet_id = sheet_id,
        agent = html_escape(&item.run.agent),
        card_id = html_escape(card_id),
        ask_line = html_escape(&ask_line),
        untriaged_marker = untriaged_marker,
        age = age,
        blocker = html_escape(blocker),
    );

    let mut curator = String::new();
    if let Some(ann) = ann {
        if let Some(situation) = &ann.situation {
            curator.push_str(&format!(
                r#"<p class="ny-situation">{}</p>"#,
                html_escape(situation)
            ));
        }
        if !ann.options.is_empty() {
            curator.push_str("<ul class=\"ny-options\">");
            for opt in &ann.options {
                curator.push_str(&format!("<li>{}</li>", html_escape(opt)));
            }
            curator.push_str("</ul>");
        }
        if let Some(reco) = &ann.recommended_answer {
            curator.push_str(&format!(
                r#"<div class="ny-reco"><b>Curator's draft answer</b> ({judge}): {reco}</div>"#,
                judge = html_escape(ann.judge.as_deref().unwrap_or("model")),
                reco = html_escape(reco)
            ));
        }
        if let Some(reason) = &ann.reason {
            curator.push_str(&format!(
                r#"<p class="ny-meta">triage: {}</p>"#,
                html_escape(reason)
            ));
        }
    }

    let evidence: Vec<String> = ann.map(|a| a.evidence_links.clone()).unwrap_or_default();
    let ev_html = if evidence.is_empty() {
        r#"<span class="ae-dim">no evidence links in this ask</span>"#.to_string()
    } else {
        evidence
            .iter()
            .map(|u| {
                let after_scheme = u.split_once("//").map_or(u.as_str(), |(_, rest)| rest);
                format!(
                    r#"<a class="ny-evidence" href="{href}">{label}</a>"#,
                    href = html_escape(u),
                    label = html_escape(after_scheme.get(..44).unwrap_or(after_scheme))
                )
            })
            .collect()
    };

    let prefill = if kind == "endorse" {
        ann.and_then(|a| a.recommended_answer.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let sheet = format!(
        r#"<div hidden id="{sheet_id}">
<p class="ny-sheet-title">{title}</p>
<p class="ny-meta">{card_id} &middot; asked {age} &middot; <a href="{board_url}#card-{card_id}">board &rarr;</a></p>
{curator}
<div class="ny-evidence-row">{ev_html}</div>
<details class="ny-raw"><summary>raw ask from {agent}</summary><div class="ny-raw-text">{question}</div></details>
<div class="ny-form">
  <textarea class="ae-input" rows="4" placeholder="Type your answer&hellip;">{prefill}</textarea>
  <button class="ae-button ae-button-compact ny-answer-btn" data-card="{card_id}" data-run="{run_id}">Answer</button>
  <span class="ny-status"></span>
</div>
</div>"#,
        sheet_id = sheet_id,
        title = html_escape(&item.card.title),
        card_id = html_escape(card_id),
        age = age,
        board_url = board_url,
        curator = curator,
        ev_html = ev_html,
        agent = html_escape(&item.run.agent),
        question = html_escape(&question_text).replace('\n', "<br>"),
        prefill = html_escape(&prefill),
        run_id = html_escape(run_id),
    );

    Rendered {
        row_and_sheet: row + &sheet,
    }
}

fn render_answered_item(item: &AnsweredItem, now: DateTime<Utc>) -> String {
    let question_text = item
        .question
        .as_ref()
        .map(|q| q.payload.clone())
        .unwrap_or_else(|| item.card.title.clone());
    let ask_line = question_text
        .lines()
        .next()
        .unwrap_or(&item.card.title)
        .to_string();
    let answered_at = item
        .answered_at
        .or_else(|| item.question.as_ref().and_then(|q| q.created_at))
        .or(item.run.created_at)
        .unwrap_or_else(|| now.timestamp());
    let age = relative_time(answered_at, now);
    let answer = item.answer.as_deref().unwrap_or("").trim();
    let answer_html = if answer.is_empty() {
        String::new()
    } else {
        format!(
            r#"<span class="ae-dim ny-meta-line">answered: {}</span>"#,
            html_escape(answer)
        )
    };
    format!(
        r#"<div class="ny-answered-row">
  <span class="ae-item">{ask_line}</span><br>
  <span class="ae-dim ny-meta-line">{agent} &middot; powder {card_id} &middot; answered {age}</span>
  {answer_html}
</div>"#,
        ask_line = html_escape(&ask_line),
        agent = html_escape(&item.run.agent),
        card_id = html_escape(&item.card.id),
        age = age,
        answer_html = answer_html,
    )
}

fn render_needs_you(
    items: &[AwaitingItem],
    answered: &[AnsweredItem],
    annotations: &HashMap<String, TriageAnnotation>,
) -> String {
    let board_url = powder_board_url().unwrap_or_else(|| "#".to_string());
    render_needs_you_with_board_url(items, answered, annotations, &board_url)
}

fn render_needs_you_with_board_url(
    items: &[AwaitingItem],
    answered: &[AnsweredItem],
    annotations: &HashMap<String, TriageAnnotation>,
    board_url: &str,
) -> String {
    let now = Utc::now();
    let mut out = format!(
        r#"<p class="ae-h">WAITING ON YOU &middot; {}</p>"#,
        items.len()
    );
    if items.is_empty() {
        out.push_str(
            r#"<p class="ny-empty ae-dim">Nothing in the fleet is awaiting your input right now.</p>"#,
        );
    } else {
        let rows = items
            .iter()
            .map(|item| {
                render_item(item, annotations.get(&item.run.id), now, board_url).row_and_sheet
            })
            .collect::<Vec<_>>()
            .join("");
        out.push_str(&format!(r#"<div class="ny-list">{rows}</div>"#));
    }

    if !answered.is_empty() {
        let rows = answered
            .iter()
            .map(|item| render_answered_item(item, now))
            .collect::<Vec<_>>()
            .join("");
        out.push_str(&format!(
            r#"<details class="ae-fold ny-answered"><summary><span class="ae-dim">ANSWERED</span><span class="ae-dim">{} from API</span></summary>{rows}</details>"#,
            answered.len()
        ));
    }

    out
}

pub(crate) async fn awaiting_input_count() -> Option<usize> {
    tokio::time::timeout(Duration::from_millis(750), fetch_awaiting_silent())
        .await
        .ok()
        .and_then(Result::ok)
        .map(|data| data.awaiting.len())
}

/// `GET /api/needs-you`. Streams a skeleton event, then a full event with
/// pre-rendered ask rows sourced from Powder's `runs/awaiting-input` (no
/// repo filter -- every repo's asks land here, closing bridge-006's gap)
/// with curator annotations read from `.ask-triage.json`. A curator refresh
/// is kicked off best-effort in the background (never blocks this response).
pub async fn needs_you_report() -> impl IntoResponse {
    trigger_triage_refresh_once();
    let stream = async_stream::stream! {
        yield Ok::<_, Infallible>(Event::default().event("skeleton").data(json!({"stage": "skeleton"}).to_string()));
        match fetch_awaiting().await {
            Ok(data) => {
                let annotations = load_triage_annotations();
                let html = render_needs_you(&data.awaiting, &data.answered, &annotations);
                yield Ok::<_, Infallible>(
                    Event::default().event("full").data(
                        json!({"stage": "full", "count": data.awaiting.len(), "html": html}).to_string(),
                    ),
                );
            }
            Err(message) => {
                yield Ok::<_, Infallible>(
                    Event::default().event("error").data(json!({"stage": "error", "message": message}).to_string()),
                );
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// Note: the client also sends `card_id` (for parity with factory-ops's own
// relay contract and easier debugging in browser devtools), but the server
// only needs `run_id` to answer -- serde ignores the extra field rather
// than rejecting it, so no `deny_unknown_fields` friction either way.
#[derive(Debug, Deserialize)]
pub struct AnswerRequest {
    pub run_id: String,
    pub answer: String,
    #[serde(default = "default_actor")]
    pub actor: String,
}

fn default_actor() -> String {
    "operator".to_string()
}

#[derive(Debug, Serialize)]
pub struct AnswerResponse {
    ok: bool,
}

/// `POST /api/needs-you/answer`. Glass's own native answer relay --
/// resolves bridge-006 by calling Powder's `answer_input` directly with no
/// repo filter, rather than proxying through an external bridge-poll
/// process scoped to one repo.
pub async fn answer(
    AxumJson(request): AxumJson<AnswerRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if request.answer.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "answer must not be empty".to_string(),
        ));
    }
    let base = powder_base_url().ok_or_else(|| {
        crate::canary::report_error(
            "glass.needs_you.answer.failed",
            "route=/api/needs-you/answer upstream=powder error_kind=missing_base_url",
        );
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "GLASS_POWDER_API_BASE_URL is not configured".to_string(),
        )
    })?;
    let key = powder_api_key().ok_or_else(|| {
        crate::canary::report_error(
            "glass.needs_you.answer.failed",
            "route=/api/needs-you/answer upstream=powder error_kind=missing_api_key",
        );
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "GLASS_POWDER_API_KEY is not configured".to_string(),
        )
    })?;
    let url = format!(
        "{}/api/v1/runs/{}/answer",
        base.trim_end_matches('/'),
        request.run_id
    );
    let response = reqwest::Client::new()
        .post(&url)
        .bearer_auth(&key)
        .json(&json!({"actor": request.actor, "answer": request.answer}))
        .send()
        .await
        .map_err(|err| {
            crate::canary::report_error(
                "glass.needs_you.answer.failed",
                "route=/api/needs-you/answer upstream=powder error_kind=transport",
            );
            (StatusCode::BAD_GATEWAY, format!("post {url}: {err}"))
        })?;
    if !response.status().is_success() {
        let status = response.status();
        crate::canary::report_error(
            "glass.needs_you.answer.failed",
            &format!(
                "route=/api/needs-you/answer upstream=powder upstream_status={} error_kind=upstream_status",
                status.as_u16()
            ),
        );
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("powder answer_input returned {status}"),
        ));
    }
    Ok(AxumJson(AnswerResponse { ok: true }))
}

const NEEDS_YOU_STYLE: &str = r#"
.ny-list { display: grid; max-width: 760px; border-top: 1px solid var(--ae-line); }
.ny-row { display: flex; align-items: center; justify-content: space-between; gap: var(--ae-space-5); padding: 0.8em 0; border-bottom: 1px solid var(--ae-line); }
.ny-row-text { min-width: 0; }
.ny-open-btn { flex: none; min-height: 32px; padding: 0.35em 1em; font-size: 13px; }
.ny-meta-line { display: block; margin-top: 0.12em; font-size: 13px; line-height: 1.45; }
.ny-untriaged { vertical-align: 0; }
.ny-empty, .ny-loading { color: var(--ae-ink-muted); padding: var(--ae-space-8) 0; }
.ny-answered { max-width: 760px; margin-top: var(--ae-space-8); }
.ny-answered-row { padding: 0.7em 0; border-top: 1px solid var(--ae-line); }
.ny-answered-row:first-of-type { border-top: 0; }
.ny-sheet-title { font-weight: var(--ae-w-medium); font-size: 16px; }
.ny-meta { font-size: 13px; color: var(--ae-ink-muted); }
.ny-situation { margin: var(--ae-space-4) 0; }
.ny-options li { margin-left: var(--ae-space-5); }
.ny-reco { border: 1px solid var(--ae-line); padding: var(--ae-space-3); margin: var(--ae-space-4) 0; }
.ny-evidence-row { display: flex; gap: var(--ae-space-2); flex-wrap: wrap; margin: var(--ae-space-3) 0; }
.ny-evidence { font-size: 13px; border: 1px solid var(--ae-line); padding: 2px 8px; }
.ny-raw { margin: var(--ae-space-4) 0; }
.ny-raw summary { cursor: pointer; color: var(--ae-ink-muted); font-size: 13px; }
.ny-form { display: flex; flex-direction: column; gap: var(--ae-space-3); margin-top: var(--ae-space-4); }
.ny-status { font-size: 13px; color: var(--ae-ink-muted); }
#ny-dialog { width: min(40em, calc(100vw - 3em)); }
@media (max-width: 36rem) {
  .ny-row { display: grid; gap: var(--ae-space-3); }
  .ny-open-btn { justify-self: start; }
}
"#;

const NEEDS_YOU_BODY: &str = r#"
<div id="ny-body"><p class="ny-loading">Loading&hellip;</p></div>
<dialog id="ny-dialog" class="ae-dialog">
  <div id="ny-dialog-body"></div>
  <div class="ae-dialog-acts">
    <button type="button" data-dialog-close class="ae-button ae-button-quiet">Close</button>
  </div>
</dialog>
"#;

const NEEDS_YOU_SCRIPT: &str = r#"
(function(){
  var bodyEl = document.getElementById('ny-body');
  var dialog = document.getElementById('ny-dialog');
  var dialogBody = document.getElementById('ny-dialog-body');
  dialog.querySelector('[data-dialog-close]').addEventListener('click', function(){ dialog.close(); });
  dialog.addEventListener('close', function(){ dialogBody.innerHTML = ''; });

  // Draft-safety (parity with factory-ops's bridge.py): drafts persist to
  // localStorage keyed by run id, restored on sheet open, cleared on
  // successful answer. guardedRefresh never reloads while a draft exists,
  // a sheet is open, or a textarea has focus/content.
  function busy() {
    if (dialog.open) return true;
    var el = document.activeElement;
    if (el && (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT')) return true;
    var tas = document.querySelectorAll('textarea');
    for (var i = 0; i < tas.length; i++) if (tas[i].value.trim()) return true;
    try {
      for (var k in localStorage) if (k.indexOf('ny-draft-') === 0 && localStorage.getItem(k)) return true;
    } catch (e) {}
    return false;
  }
  function guardedRefresh() {
    if (!busy()) { load(); return; }
    setTimeout(guardedRefresh, 15000);
  }
  setTimeout(guardedRefresh, 60000);

  document.addEventListener('input', function(e){
    var t = e.target;
    if (t.tagName !== 'TEXTAREA') return;
    var btn = t.parentElement && t.parentElement.querySelector('.ny-answer-btn');
    if (!btn) return;
    try { localStorage.setItem('ny-draft-' + btn.getAttribute('data-run'), t.value); } catch (err) {}
  });
  function restoreDraft(scope) {
    var btn = scope.querySelector('.ny-answer-btn');
    var ta = scope.querySelector('textarea');
    if (!btn || !ta) return;
    try {
      var d = localStorage.getItem('ny-draft-' + btn.getAttribute('data-run'));
      if (d && !ta.value) ta.value = d;
    } catch (err) {}
  }
  function clearDraft(runId) {
    try { localStorage.removeItem('ny-draft-' + runId); } catch (err) {}
  }
  function updateRailCount(count) {
    var link = document.querySelector('.ae-rail a[href="/needs-you"]');
    if (!link || typeof count !== 'number') return;
    link.textContent = 'Needs you · ' + count;
  }

  function wireAnswerButtons(scope) {
    scope.querySelectorAll('.ny-answer-btn').forEach(function(btn){
      if (btn._wired) return;
      btn._wired = true;
      btn.addEventListener('click', async function(){
        var cardId = btn.dataset.card, runId = btn.dataset.run;
        var form = btn.closest('.ny-form');
        var ta = form.querySelector('textarea');
        var status = form.querySelector('.ny-status');
        var answer = ta.value.trim();
        if (!answer) { status.textContent = 'enter an answer first'; return; }
        btn.disabled = true;
        status.textContent = 'sending…';
        try {
          var res = await fetch('/api/needs-you/answer', {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({card_id: cardId, run_id: runId, answer: answer, actor: 'operator'})
          });
          if (!res.ok) {
            status.textContent = 'error: ' + (await res.text()).slice(0, 160);
            btn.disabled = false;
            return;
          }
          status.textContent = 'answered — refreshing…';
          clearDraft(runId);
          setTimeout(function(){ dialog.close(); load(); }, 600);
        } catch (err) {
          status.textContent = 'error: ' + err;
          btn.disabled = false;
        }
      });
    });
  }

  function wireRows(scope) {
    scope.querySelectorAll('[data-sheet]').forEach(function(invoker){
      invoker.addEventListener('click', function(){
        var src = document.getElementById(invoker.dataset.sheet);
        if (!src) return;
        dialogBody.innerHTML = src.innerHTML;
        wireAnswerButtons(dialogBody);
        restoreDraft(dialogBody);
        dialog.showModal();
      });
    });
  }

  function load() {
    bodyEl.innerHTML = '<p class="ny-loading">Loading&hellip;</p>';
    var es = new EventSource('/api/needs-you');
    es.addEventListener('skeleton', function(){
      bodyEl.innerHTML = '<p class="ny-loading">Checking the queue&hellip;</p>';
    });
    es.addEventListener('full', function(ev){
      var data = JSON.parse(ev.data);
      bodyEl.innerHTML = data.html;
      updateRailCount(data.count);
      wireRows(bodyEl);
      es.close();
    });
    es.addEventListener('error', function(ev){
      try {
        var data = JSON.parse(ev.data);
        bodyEl.innerHTML = '<p class="ny-loading">' + data.message + '</p>';
      } catch (e) {
        bodyEl.innerHTML = '<p class="ny-loading">Needs You unavailable.</p>';
      }
      es.close();
    });
  }

  load();
})();
"#;

pub async fn needs_you_shell() -> impl IntoResponse {
    let count = awaiting_input_count().await;
    axum::response::Html(shell::render_shell(shell::Shell {
        title: "Glass - Needs You",
        active: Some(shell::Place::NeedsYou),
        needs_you_count: count,
        sanctum_url: &sanctum_url(),
        styles: NEEDS_YOU_STYLE,
        body: NEEDS_YOU_BODY,
        scripts: NEEDS_YOU_SCRIPT,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn awaiting_item(card_id: &str, run_id: &str, agent: &str, payload: &str) -> AwaitingItem {
        AwaitingItem {
            card: CardBrief {
                id: card_id.to_string(),
                title: format!("{card_id} title"),
                repo: Some("glass".to_string()),
                priority: Some("p1".to_string()),
            },
            question: Some(QuestionPayload {
                payload: payload.to_string(),
                created_at: Some(Utc::now().timestamp() - 3600),
            }),
            run: RunInfo {
                id: run_id.to_string(),
                agent: agent.to_string(),
                created_at: Some(Utc::now().timestamp() - 3600),
            },
        }
    }

    #[test]
    fn kind_of_defaults_to_decide_when_untriaged() {
        assert_eq!(kind_of(None), "decide");
    }

    #[test]
    fn kind_of_reads_the_curator_annotation() {
        let ann = TriageAnnotation {
            kind: Some("question".to_string()),
            ..Default::default()
        };
        assert_eq!(kind_of(Some(&ann)), "question");
    }

    #[test]
    fn render_needs_you_uses_fig5_waiting_rows() {
        let items = vec![
            awaiting_item("glass-1", "run-1", "team-lead", "DECIDE: pick a path"),
            awaiting_item("glass-2", "run-2", "reviewer", "ACT: approve the fixture"),
        ];
        let mut annotations = HashMap::new();
        annotations.insert(
            "run-1".to_string(),
            TriageAnnotation {
                kind: Some("decide".to_string()),
                run: Some("run-1".to_string()),
                ..Default::default()
            },
        );
        annotations.insert(
            "run-2".to_string(),
            TriageAnnotation {
                kind: Some("act".to_string()),
                run: Some("run-2".to_string()),
                ..Default::default()
            },
        );
        let html = render_needs_you(&items, &[], &annotations);
        assert!(html.contains(r#"<p class="ae-h">WAITING ON YOU &middot; 2</p>"#));
        assert!(html.contains(r#"<span class="ae-item">DECIDE: pick a path</span>"#));
        assert!(html.contains("team-lead &middot; powder glass-1 &middot; asked "));
        assert!(
            html.contains(r#"<button type="button" class="ae-button ae-button-quiet ny-open-btn""#)
        );
        assert!(!html.contains("ny-chip"));
        assert!(!html.contains("ny-litter"));
    }

    #[test]
    fn render_needs_you_marks_untriaged_items_when_no_annotation_exists() {
        let items = vec![awaiting_item("glass-3", "run-3", "team-lead", "some ask")];
        let html = render_needs_you(&items, &[], &HashMap::new());
        assert!(html.contains("untriaged"));
        assert!(html.contains(r#"<span class="ae-item">some ask</span>"#));
    }

    #[test]
    fn render_needs_you_reports_an_explicit_empty_state() {
        let html = render_needs_you(&[], &[], &HashMap::new());
        assert!(html.contains("Nothing in the fleet is awaiting your input"));
    }

    #[test]
    fn render_needs_you_only_shows_answered_fold_from_api_data() {
        let no_answered = render_needs_you(&[], &[], &HashMap::new());
        assert!(!no_answered.contains("ANSWERED"));

        let answered = vec![AnsweredItem {
            card: CardBrief {
                id: "glass-1".to_string(),
                title: "Shell work".to_string(),
                repo: Some("glass".to_string()),
                priority: Some("p1".to_string()),
            },
            question: Some(QuestionPayload {
                payload: "DECIDE: keep it active?".to_string(),
                created_at: Some(Utc::now().timestamp() - 120),
            }),
            run: RunInfo {
                id: "run-1".to_string(),
                agent: "glass-931-codex".to_string(),
                created_at: Some(Utc::now().timestamp() - 300),
            },
            answer: Some("yes".to_string()),
            answered_at: Some(Utc::now().timestamp() - 60),
        }];
        let html = render_needs_you(&[], &answered, &HashMap::new());
        assert!(html.contains(r#"<details class="ae-fold ny-answered">"#));
        assert!(html.contains("ANSWERED"));
        assert!(html.contains("1 from API"));
        assert!(html.contains("answered: yes"));
    }

    #[test]
    fn render_needs_you_uses_operator_facing_board_url_when_configured() {
        let items = vec![awaiting_item(
            "glass-922",
            "run-922",
            "team-lead",
            "pick the right board URL",
        )];
        let board_url = powder_board_url_from(
            Some("https://powder.sanctum.tailnet"),
            Some("http://127.0.0.1:4175"),
        )
        .expect("board URL");
        let html = render_needs_you_with_board_url(&items, &[], &HashMap::new(), &board_url);

        assert!(html.contains(r#"href="https://powder.sanctum.tailnet/board#card-glass-922""#));
        assert!(!html.contains("http://127.0.0.1:4175"));
    }

    #[test]
    fn board_url_falls_back_to_api_base_when_operator_url_is_unset() {
        assert_eq!(
            powder_board_url_from(None, Some("http://127.0.0.1:4175/")),
            Some("http://127.0.0.1:4175/board".to_string())
        );
    }

    #[test]
    fn board_url_does_not_duplicate_a_configured_board_path() {
        assert_eq!(
            powder_board_url_from(Some("https://powder.sanctum.tailnet/board/"), None),
            Some("https://powder.sanctum.tailnet/board".to_string())
        );
    }

    #[test]
    fn awaiting_fetch_url_keeps_using_the_api_base_when_board_url_differs() {
        let api_base = "http://127.0.0.1:4175/";
        let board_url =
            powder_board_url_from(Some("https://powder.sanctum.tailnet"), Some(api_base))
                .expect("board URL");

        assert_eq!(board_url, "https://powder.sanctum.tailnet/board");
        assert_eq!(
            awaiting_input_url_from_api_base(api_base),
            "http://127.0.0.1:4175/api/v1/runs/awaiting-input?limit=100"
        );
    }

    #[test]
    fn load_triage_annotations_keys_by_run_id() {
        let dir = std::env::temp_dir().join(format!("glass-triage-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".ask-triage.json");
        std::fs::write(
            &path,
            r#"{"key1": {"kind": "decide", "run": "run-9", "card": "glass-9"}}"#,
        )
        .unwrap();
        // SAFETY: test-local env var pointing to a test-local temp file;
        // not touching any real secret or shared process-wide state a
        // concurrent test could race on.
        unsafe {
            std::env::set_var("GLASS_ASK_TRIAGE_CACHE", &path);
        }
        let annotations = load_triage_annotations();
        unsafe {
            std::env::remove_var("GLASS_ASK_TRIAGE_CACHE");
        }
        std::fs::remove_dir_all(&dir).ok();
        assert!(annotations.contains_key("run-9"));
    }
}
