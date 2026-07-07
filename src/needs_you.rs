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

fn powder_base_url() -> Option<String> {
    std::env::var("GLASS_POWDER_API_BASE_URL")
        .ok()
        .filter(|v| !v.is_empty())
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

async fn fetch_awaiting() -> Result<Vec<AwaitingItem>, String> {
    let base = powder_base_url()
        .ok_or_else(|| "GLASS_POWDER_API_BASE_URL is not configured".to_string())?;
    let key =
        powder_api_key().ok_or_else(|| "GLASS_POWDER_API_KEY is not configured".to_string())?;
    let url = format!(
        "{}/api/v1/runs/awaiting-input?limit=100",
        base.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&key)
        .send()
        .await
        .map_err(|err| format!("fetch {url}: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "fetch {url}: upstream returned {}",
            response.status()
        ));
    }
    let mut items = response
        .json::<AwaitingResponse>()
        .await
        .map(|body| body.awaiting)
        .map_err(|err| format!("parse {url}: {err}"))?;
    sort_awaiting(&mut items);
    Ok(items)
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
            let _ = tokio::time::timeout(
                Duration::from_secs(90),
                tokio::process::Command::new("python3")
                    .arg(&script)
                    .arg("--quiet")
                    .output(),
            )
            .await;
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

fn chip_for(kind: &str) -> &'static str {
    match kind {
        "question" => "?",
        "act" => "ACT",
        "endorse" => "CLOSE ME",
        _ => "DECIDE",
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

/// One rendered row (the compact rail entry) + its sheet detail markup,
/// tagged with its curator kind for grouping.
struct Rendered {
    kind: &'static str,
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
        r#"<span class="ny-untriaged" title="curator has not judged this ask yet">untriaged</span>"#
    } else {
        ""
    };
    let sheet_id = format!("ny-sheet-{}", html_escape(card_id));

    let row = format!(
        r#"<button type="button" class="ny-row ny-row-{kind}" data-sheet="{sheet_id}">
  <span class="ny-chip ny-chip-{kind}">{chip}</span>
  <span class="ny-id">{card_id}</span>
  <span class="ny-title">{ask_line}</span>
  {untriaged_marker}<span class="ny-age">{age}</span>
</button>"#,
        kind = kind,
        chip = chip_for(kind),
        sheet_id = sheet_id,
        card_id = html_escape(card_id),
        ask_line = html_escape(&ask_line),
        untriaged_marker = untriaged_marker,
        age = age,
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
        r#"<span class="ny-dim">no evidence links in this ask</span>"#.to_string()
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
        kind,
        row_and_sheet: row + &sheet,
    }
}

fn render_needs_you(
    items: &[AwaitingItem],
    annotations: &HashMap<String, TriageAnnotation>,
) -> String {
    if items.is_empty() {
        return r#"<p class="ny-dim">Nothing in the fleet is awaiting your input right now.</p>"#
            .to_string();
    }
    let now = Utc::now();
    let board_url = powder_base_url()
        .map(|base| format!("{}/board", base.trim_end_matches('/')))
        .unwrap_or_else(|| "#".to_string());

    let rendered: Vec<Rendered> = items
        .iter()
        .map(|item| render_item(item, annotations.get(&item.run.id), now, &board_url))
        .collect();

    let mut out = String::new();
    for (kind, label) in [
        ("decide", "Decide"),
        ("question", "Clarify"),
        ("act", "Only you can"),
    ] {
        let group: Vec<&str> = rendered
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| r.row_and_sheet.as_str())
            .collect();
        if !group.is_empty() {
            out.push_str(&format!(r#"<p class="ny-group">{label}</p>"#));
            out.push_str(&group.join(""));
        }
    }
    let litter: Vec<&str> = rendered
        .iter()
        .filter(|r| r.kind == "endorse")
        .map(|r| r.row_and_sheet.as_str())
        .collect();
    if !litter.is_empty() {
        out.push_str(&format!(
            r#"<details class="ny-litter"><summary>{} endorsement ask(s) &mdash; answer &ldquo;ship it&rdquo;</summary>{}</details>"#,
            litter.len(),
            litter.join("")
        ));
    }
    out
}

/// `GET /api/needs-you`. Streams a skeleton event, then a full event with
/// pre-rendered rail HTML: kind-grouped rows sourced from Powder's
/// `runs/awaiting-input` (no repo filter -- every repo's asks land here,
/// closing bridge-006's gap) with curator annotations read from
/// `.ask-triage.json`. A curator refresh is kicked off best-effort in the
/// background (never blocks this response).
pub async fn needs_you_report() -> impl IntoResponse {
    trigger_triage_refresh_once();
    let stream = async_stream::stream! {
        yield Ok::<_, Infallible>(Event::default().event("skeleton").data(json!({"stage": "skeleton"}).to_string()));
        match fetch_awaiting().await {
            Ok(items) => {
                let annotations = load_triage_annotations();
                let html = render_needs_you(&items, &annotations);
                yield Ok::<_, Infallible>(
                    Event::default().event("full").data(
                        json!({"stage": "full", "count": items.len(), "html": html}).to_string(),
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
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "GLASS_POWDER_API_BASE_URL is not configured".to_string(),
        )
    })?;
    let key = powder_api_key().ok_or_else(|| {
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
        .map_err(|err| (StatusCode::BAD_GATEWAY, format!("post {url}: {err}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("powder answer_input returned {status}: {body}"),
        ));
    }
    Ok(AxumJson(AnswerResponse { ok: true }))
}

const NEEDS_YOU_SHELL: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Glass — Needs You</title>
<link rel="stylesheet" href="/aesthetic.css">
<script>
try {
  var m = localStorage.getItem('ae-mode');
  if (m === 'dark' || m === 'light') {
    document.documentElement.classList.add(m);
    document.documentElement.style.colorScheme = m;
  }
} catch (e) {}
</script>
<style>
.ny-shell { max-width: 760px; margin: 0 auto; padding: var(--ae-space-6) var(--ae-space-5); }
.ny-group { font-family: var(--ae-font-mono); font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--ae-ink-muted); margin: var(--ae-space-6) 0 var(--ae-space-3); }
.ny-row { display: flex; align-items: center; gap: var(--ae-space-3); width: 100%; text-align: left; padding: var(--ae-space-3) var(--ae-space-4); border: 1px solid var(--ae-line); background: var(--ae-surface); color: var(--ae-ink); cursor: pointer; margin-bottom: var(--ae-space-2); }
.ny-chip { font-family: var(--ae-font-mono); font-size: 11px; font-weight: var(--ae-w-medium); padding: 2px 6px; border: 1px solid var(--ae-line); }
.ny-id { font-family: var(--ae-font-mono); font-size: 12px; color: var(--ae-ink-muted); }
.ny-title { flex: 1; }
.ny-age { font-size: 12px; color: var(--ae-ink-muted); white-space: nowrap; }
.ny-untriaged { font-size: 11px; color: var(--ae-ink-muted); }
.ny-litter summary { cursor: pointer; color: var(--ae-ink-muted); font-size: 13px; margin-top: var(--ae-space-5); }
.ny-dim { color: var(--ae-ink-muted); padding: var(--ae-space-8) 0; text-align: center; }
.ny-sheet-title { font-weight: var(--ae-w-medium); font-size: 16px; }
.ny-meta { font-size: 12px; color: var(--ae-ink-muted); }
.ny-situation { margin: var(--ae-space-4) 0; }
.ny-options li { margin-left: var(--ae-space-5); }
.ny-reco { border: 1px solid var(--ae-line); padding: var(--ae-space-3); margin: var(--ae-space-4) 0; }
.ny-evidence-row { display: flex; gap: var(--ae-space-2); flex-wrap: wrap; margin: var(--ae-space-3) 0; }
.ny-evidence { font-size: 12px; border: 1px solid var(--ae-line); padding: 2px 8px; }
.ny-raw { margin: var(--ae-space-4) 0; }
.ny-raw summary { cursor: pointer; color: var(--ae-ink-muted); font-size: 13px; }
.ny-form { display: flex; flex-direction: column; gap: var(--ae-space-3); margin-top: var(--ae-space-4); }
.ny-loading { color: var(--ae-ink-muted); padding: var(--ae-space-8) 0; text-align: center; }
#ny-dialog { border: 1px solid var(--ae-line); padding: var(--ae-space-6); max-width: 640px; width: 90vw; }
#ny-dialog::backdrop { background: rgba(0,0,0,0.4); }
</style>
</head>
<body>
<div class="ae-shell">
  <aside class="ae-rail">
    <a class="ae-logo ae-logo-compact" href="/">
      <span class="ae-app-mark"><svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M11 6 8 9"></path><path d="m16 7-8 8"></path><rect x="4" y="2" width="16" height="20"></rect></svg></span>
      <span class="ae-name">Glass</span>
    </a>
    <p class="ae-h">Report</p>
    <a href="/rep1">Fleet report</a>
    <a href="/backlog/glass">Backlog</a>
    <a href="/needs-you" id="ny-nav-active">Needs You</a>
    <a href="/">Raw live feed</a>
  </aside>
  <main class="ae-desk">
    <div class="ny-shell">
      <h1 class="ae-strong">Needs You</h1>
      <div id="ny-body"><p class="ny-loading">Loading&hellip;</p></div>
    </div>
  </main>
</div>
<dialog id="ny-dialog">
  <div id="ny-dialog-body"></div>
  <button type="button" data-dialog-close class="ae-button ae-button-compact">Close</button>
</dialog>
<script>
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
</script>
</body>
</html>"#;

pub async fn needs_you_shell() -> impl IntoResponse {
    axum::response::Html(NEEDS_YOU_SHELL.to_string())
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
    fn render_needs_you_groups_by_kind_and_collapses_endorse() {
        let items = vec![
            awaiting_item("glass-1", "run-1", "team-lead", "DECIDE: pick a path"),
            awaiting_item(
                "glass-2",
                "run-2",
                "team-lead",
                "please look at this, ship it?",
            ),
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
                kind: Some("endorse".to_string()),
                run: Some("run-2".to_string()),
                recommended_answer: Some("ship it".to_string()),
                ..Default::default()
            },
        );
        let html = render_needs_you(&items, &annotations);
        assert!(html.contains(r#"<p class="ny-group">Decide</p>"#));
        assert!(html.contains("ny-litter"));
        assert!(html.contains("1 endorsement ask(s)"));
        assert!(html.contains("glass-1"));
        assert!(html.contains("glass-2"));
    }

    #[test]
    fn render_needs_you_marks_untriaged_items_when_no_annotation_exists() {
        let items = vec![awaiting_item("glass-3", "run-3", "team-lead", "some ask")];
        let html = render_needs_you(&items, &HashMap::new());
        assert!(html.contains("untriaged"));
        assert!(
            html.contains(r#"<p class="ny-group">Decide</p>"#),
            "untriaged defaults to decide"
        );
    }

    #[test]
    fn render_needs_you_reports_an_explicit_empty_state() {
        let html = render_needs_you(&[], &HashMap::new());
        assert!(html.contains("Nothing in the fleet is awaiting your input"));
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
