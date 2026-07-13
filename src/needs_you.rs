//! Needs You: the operator's Bitterblossom HITL queue as a native Glass view.
//! Bitterblossom owns ask state and answer/resume semantics; Glass is only an
//! operator-facing projection and relay over its authenticated ask API.

use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::extract::Json as AxumJson;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{sanctum_url, shell};

fn bitterblossom_base_url() -> Option<String> {
    std::env::var("GLASS_BITTERBLOSSOM_API_BASE_URL")
        .ok()
        .filter(|v| !v.is_empty())
}

fn bitterblossom_dashboard_url() -> Option<String> {
    let operator_url = std::env::var("GLASS_BITTERBLOSSOM_DASHBOARD_URL")
        .ok()
        .filter(|v| !v.is_empty());
    let api_base = bitterblossom_base_url();
    operator_url
        .or(api_base)
        .map(|base| base.trim_end_matches('/').to_string())
}

fn bitterblossom_api_key() -> Option<String> {
    std::env::var("GLASS_BITTERBLOSSOM_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
}

#[derive(Debug, Deserialize, Clone)]
struct OpenAsk {
    id: String,
    run_id: String,
    task: String,
    kind: String,
    question: String,
    context: Option<String>,
    blocking: bool,
    state: String,
    created_at: String,
    answer: Option<String>,
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
}

async fn fetch_awaiting() -> Result<Vec<OpenAsk>, String> {
    fetch_awaiting_with_reporting(true).await
}

async fn fetch_awaiting_silent() -> Result<Vec<OpenAsk>, String> {
    fetch_awaiting_with_reporting(false).await
}

async fn fetch_awaiting_with_reporting(report_errors: bool) -> Result<Vec<OpenAsk>, String> {
    let base = match bitterblossom_base_url() {
        Some(base) => base,
        None => {
            if report_errors {
                crate::canary::report_error(
                    "glass.needs_you.fetch.failed",
                    "route=/api/needs-you upstream=bitterblossom error_kind=missing_base_url",
                );
            }
            return Err("GLASS_BITTERBLOSSOM_API_BASE_URL is not configured".to_string());
        }
    };
    let key = match bitterblossom_api_key() {
        Some(key) => key,
        None => {
            if report_errors {
                crate::canary::report_error(
                    "glass.needs_you.fetch.failed",
                    "route=/api/needs-you upstream=bitterblossom error_kind=missing_api_key",
                );
            }
            return Err("GLASS_BITTERBLOSSOM_API_KEY is not configured".to_string());
        }
    };
    let url = asks_url_from_api_base(&base);
    let response = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&key)
        .send()
        .await
        .map_err(|err| {
            if report_errors {
                crate::canary::report_error(
                    "glass.needs_you.fetch.failed",
                    "route=/api/needs-you upstream=bitterblossom error_kind=transport",
                );
            }
            format!("fetch {url}: {err}")
        })?;
    if !response.status().is_success() {
        if report_errors {
            crate::canary::report_error(
                "glass.needs_you.fetch.failed",
                &format!(
                    "route=/api/needs-you upstream=bitterblossom upstream_status={} error_kind=upstream_status",
                    response.status().as_u16()
                ),
            );
        }
        return Err(format!(
            "fetch {url}: upstream returned {}",
            response.status()
        ));
    }
    let mut asks = response.json::<Vec<OpenAsk>>().await.map_err(|err| {
        if report_errors {
            crate::canary::report_error(
                "glass.needs_you.fetch.failed",
                "route=/api/needs-you upstream=bitterblossom error_kind=parse",
            );
        }
        format!("parse {url}: {err}")
    })?;
    sort_awaiting(&mut asks);
    Ok(asks)
}

fn asks_url_from_api_base(base: &str) -> String {
    format!("{}/api/asks", base.trim_end_matches('/'))
}

/// Blocking asks first, then stable oldest-first order. These are declared BB
/// fields, so Glass does not need to infer urgency from prose.
fn sort_awaiting(items: &mut [OpenAsk]) {
    items.sort_by(|a, b| {
        b.blocking
            .cmp(&a.blocking)
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn kind_of(ann: Option<&TriageAnnotation>, declared: &str) -> &'static str {
    match ann.and_then(|a| a.kind.as_deref()) {
        Some("question") => "question",
        Some("act") => "act",
        Some("endorse" | "approval") => "endorse",
        _ => match declared {
            "question" => "question",
            "act" => "act",
            "endorse" | "approval" => "endorse",
            _ => "decide",
        },
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

fn relative_time(ts: &str, now: DateTime<Utc>) -> String {
    let Ok(then) = DateTime::parse_from_rfc3339(ts) else {
        return "?".to_string();
    };
    let delta = (now - then.with_timezone(&Utc)).num_seconds();
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
    item: &OpenAsk,
    ann: Option<&TriageAnnotation>,
    now: DateTime<Utc>,
    dashboard_url: &str,
) -> Rendered {
    let ask_id = &item.id;
    let question_text = &item.question;
    let kind = kind_of(ann, &item.kind);
    let ask_line = ann.and_then(|a| a.ask_line.clone()).unwrap_or_else(|| {
        question_text
            .lines()
            .next()
            .unwrap_or(&item.task)
            .to_string()
    });
    let age = relative_time(&item.created_at, now);
    let untriaged_marker = if ann.is_none() {
        r#" <span class="ae-tag ae-tag-bare ny-untriaged" title="curator has not judged this ask yet">untriaged</span>"#
    } else {
        ""
    };
    let blocker = ann
        .and_then(|a| a.situation.as_deref())
        .filter(|value| !value.trim().is_empty())
        .or(item.context.as_deref())
        .unwrap_or(item.task.as_str());
    let sheet_id = format!("ny-sheet-{}", html_escape(ask_id));

    let row = format!(
        r#"<div class="ny-row ny-row-{kind}">
  <span class="ny-row-text">
    <span class="ae-item">{ask_line}</span>{untriaged_marker}<br>
    <span class="ae-dim ny-meta-line">{task} &middot; bitterblossom {ask_id} &middot; {ask_kind} &middot; {state} &middot; asked {age} &middot; {blocker}</span>
  </span>
  <button type="button" class="ae-button ae-button-quiet ny-open-btn" data-sheet="{sheet_id}">Answer</button>
</div>"#,
        kind = kind,
        sheet_id = sheet_id,
        task = html_escape(&item.task),
        ask_id = html_escape(ask_id),
        ask_kind = html_escape(&item.kind),
        state = html_escape(&item.state),
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

    let prefill = if let Some(answer) = item.answer.as_ref() {
        answer.clone()
    } else if kind == "endorse" {
        ann.and_then(|a| a.recommended_answer.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let sheet = format!(
        r#"<div hidden id="{sheet_id}">
<p class="ny-sheet-title">{title}</p>
<p class="ny-meta">{ask_id} &middot; run {run_id} &middot; asked {age} &middot; <a href="{dashboard_url}">Bitterblossom &rarr;</a></p>
{curator}
<div class="ny-evidence-row">{ev_html}</div>
<details class="ny-raw"><summary>raw ask from {task}</summary><div class="ny-raw-text">{question}</div></details>
<div class="ny-form">
  <textarea class="ae-input" rows="4" placeholder="Type your answer&hellip;">{prefill}</textarea>
  <button class="ae-button ae-button-compact ny-answer-btn" data-ask="{ask_id}">Answer</button>
  <span class="ny-status"></span>
</div>
</div>"#,
        sheet_id = sheet_id,
        title = html_escape(&item.task),
        ask_id = html_escape(ask_id),
        run_id = html_escape(&item.run_id),
        age = age,
        dashboard_url = html_escape(dashboard_url),
        curator = curator,
        ev_html = ev_html,
        task = html_escape(&item.task),
        question = html_escape(question_text).replace('\n', "<br>"),
        prefill = html_escape(&prefill),
    );

    Rendered {
        row_and_sheet: row + &sheet,
    }
}

fn render_needs_you(items: &[OpenAsk], annotations: &HashMap<String, TriageAnnotation>) -> String {
    let dashboard_url = bitterblossom_dashboard_url().unwrap_or_else(|| "#".to_string());
    render_needs_you_with_dashboard_url(items, annotations, &dashboard_url)
}

fn render_needs_you_with_dashboard_url(
    items: &[OpenAsk],
    annotations: &HashMap<String, TriageAnnotation>,
    dashboard_url: &str,
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
                render_item(item, annotations.get(&item.run_id), now, dashboard_url).row_and_sheet
            })
            .collect::<Vec<_>>()
            .join("");
        out.push_str(&format!(r#"<div class="ny-list">{rows}</div>"#));
    }

    out
}

pub(crate) async fn needs_you_count() -> Option<usize> {
    tokio::time::timeout(Duration::from_millis(750), fetch_awaiting_silent())
        .await
        .ok()
        .and_then(Result::ok)
        .map(|asks| asks.len())
}

/// `GET /api/needs-you`. Streams a skeleton event, then a full event with
/// pre-rendered ask rows sourced from Bitterblossom's unanswered asks
/// using Bitterblossom's declared ask fields. Glass neither launches the
/// legacy Powder-backed curator nor consumes its run-keyed cache.
pub async fn needs_you_report() -> impl IntoResponse {
    let stream = async_stream::stream! {
        yield Ok::<_, Infallible>(Event::default().event("skeleton").data(json!({"stage": "skeleton"}).to_string()));
        match fetch_awaiting().await {
            Ok(asks) => {
                let html = render_needs_you(&asks, &HashMap::new());
                yield Ok::<_, Infallible>(
                    Event::default().event("full").data(
                        json!({"stage": "full", "count": asks.len(), "html": html}).to_string(),
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

#[derive(Debug, Deserialize)]
pub struct AnswerRequest {
    pub ask_id: String,
    pub answer: String,
}

#[derive(Debug, Serialize)]
pub struct AnswerResponse {
    ok: bool,
}

fn answer_url_from_api_base(base: &str, ask_id: &str) -> String {
    format!("{}/api/asks/{ask_id}/answer", base.trim_end_matches('/'))
}

/// `POST /api/needs-you/answer`. Glass's own native answer relay --
/// relays the operator answer to Bitterblossom, which owns both recording the
/// answer and resuming a parked workflow when required.
pub async fn answer(
    AxumJson(request): AxumJson<AnswerRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if request.answer.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "answer must not be empty".to_string(),
        ));
    }
    let base = bitterblossom_base_url().ok_or_else(|| {
        crate::canary::report_error(
            "glass.needs_you.answer.failed",
            "route=/api/needs-you/answer upstream=bitterblossom error_kind=missing_base_url",
        );
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "GLASS_BITTERBLOSSOM_API_BASE_URL is not configured".to_string(),
        )
    })?;
    let key = bitterblossom_api_key().ok_or_else(|| {
        crate::canary::report_error(
            "glass.needs_you.answer.failed",
            "route=/api/needs-you/answer upstream=bitterblossom error_kind=missing_api_key",
        );
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "GLASS_BITTERBLOSSOM_API_KEY is not configured".to_string(),
        )
    })?;
    let url = answer_url_from_api_base(&base, &request.ask_id);
    let response = reqwest::Client::new()
        .post(&url)
        .bearer_auth(&key)
        .json(&json!({"answered_by": "operator", "answer": request.answer}))
        .send()
        .await
        .map_err(|err| {
            crate::canary::report_error(
                "glass.needs_you.answer.failed",
                "route=/api/needs-you/answer upstream=bitterblossom error_kind=transport",
            );
            (StatusCode::BAD_GATEWAY, format!("post {url}: {err}"))
        })?;
    if !response.status().is_success() {
        let status = response.status();
        crate::canary::report_error(
            "glass.needs_you.answer.failed",
            &format!(
                "route=/api/needs-you/answer upstream=bitterblossom upstream_status={} error_kind=upstream_status",
                status.as_u16()
            ),
        );
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("bitterblossom answer_ask returned {status}"),
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
  // localStorage keyed by ask id, restored on sheet open, cleared on
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
    try { localStorage.setItem('ny-draft-' + btn.getAttribute('data-ask'), t.value); } catch (err) {}
  });
  function restoreDraft(scope) {
    var btn = scope.querySelector('.ny-answer-btn');
    var ta = scope.querySelector('textarea');
    if (!btn || !ta) return;
    try {
      var d = localStorage.getItem('ny-draft-' + btn.getAttribute('data-ask'));
      if (d && !ta.value) ta.value = d;
    } catch (err) {}
  }
  function clearDraft(askId) {
    try { localStorage.removeItem('ny-draft-' + askId); } catch (err) {}
  }
  function updateRailCount(count) {
    if (typeof count !== 'number') return;
    document.querySelectorAll('[data-needs-you-count]').forEach(function(el){
      el.textContent = String(count);
    });
    document.querySelectorAll('a[href="/needs-you"][aria-label^="Needs you"]').forEach(function(link){
      link.setAttribute('aria-label', 'Needs you ' + count);
    });
  }

  function wireAnswerButtons(scope) {
    scope.querySelectorAll('.ny-answer-btn').forEach(function(btn){
      if (btn._wired) return;
      btn._wired = true;
      btn.addEventListener('click', async function(){
        var askId = btn.dataset.ask;
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
            body: JSON.stringify({ask_id: askId, answer: answer})
          });
          if (!res.ok) {
            status.textContent = 'error: ' + (await res.text()).slice(0, 160);
            btn.disabled = false;
            return;
          }
          status.textContent = 'answered — refreshing…';
          clearDraft(askId);
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
    let count = needs_you_count().await;
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

    fn open_ask(ask_id: &str, run_id: &str, task: &str, question: &str) -> OpenAsk {
        OpenAsk {
            id: ask_id.to_string(),
            run_id: run_id.to_string(),
            task: task.to_string(),
            kind: "decision".to_string(),
            question: question.to_string(),
            context: Some(format!("{task} needs a decision")),
            blocking: true,
            state: "open".to_string(),
            created_at: (Utc::now() - chrono::Duration::hours(1)).to_rfc3339(),
            answer: None,
        }
    }

    #[test]
    fn kind_of_defaults_to_decide_when_untriaged() {
        assert_eq!(kind_of(None, "decision"), "decide");
    }

    #[test]
    fn kind_of_reads_the_curator_annotation() {
        let ann = TriageAnnotation {
            kind: Some("question".to_string()),
            ..Default::default()
        };
        assert_eq!(kind_of(Some(&ann), "decision"), "question");
    }

    #[test]
    fn kind_of_maps_declared_bb_approval_to_endorse() {
        assert_eq!(kind_of(None, "approval"), "endorse");
    }

    #[test]
    fn render_needs_you_uses_fig5_waiting_rows() {
        let items = vec![
            open_ask("ask-1", "run-1", "team-lead", "DECIDE: pick a path"),
            open_ask("ask-2", "run-2", "reviewer", "ACT: approve the fixture"),
        ];
        let mut annotations = HashMap::new();
        annotations.insert(
            "run-1".to_string(),
            TriageAnnotation {
                kind: Some("decide".to_string()),
                ..Default::default()
            },
        );
        annotations.insert(
            "run-2".to_string(),
            TriageAnnotation {
                kind: Some("act".to_string()),
                ..Default::default()
            },
        );
        let html = render_needs_you_with_dashboard_url(
            &items,
            &annotations,
            "https://bitterblossom.example.test",
        );
        assert!(html.contains(r#"<p class="ae-h">WAITING ON YOU &middot; 2</p>"#));
        assert!(html.contains(r#"<span class="ae-item">DECIDE: pick a path</span>"#));
        assert!(
            html.contains("team-lead &middot; bitterblossom ask-1 &middot; decision &middot; open")
        );
        assert!(html.contains(r#"data-ask="ask-1""#));
        assert!(
            html.contains(r#"<button type="button" class="ae-button ae-button-quiet ny-open-btn""#)
        );
        assert!(!html.contains("ny-chip"));
        assert!(!html.contains("ny-litter"));
    }

    #[test]
    fn render_needs_you_marks_untriaged_items_when_no_annotation_exists() {
        let items = vec![open_ask("ask-3", "run-3", "team-lead", "some ask")];
        let html = render_needs_you_with_dashboard_url(&items, &HashMap::new(), "#");
        assert!(html.contains("untriaged"));
        assert!(html.contains(r#"<span class="ae-item">some ask</span>"#));
    }

    #[test]
    fn render_needs_you_reports_an_explicit_empty_state() {
        let html = render_needs_you_with_dashboard_url(&[], &HashMap::new(), "#");
        assert!(html.contains("Nothing in the fleet is awaiting your input"));
    }

    #[test]
    fn fetch_and_answer_urls_use_bitterblossom_asks() {
        assert_eq!(
            asks_url_from_api_base("http://127.0.0.1:8080/"),
            "http://127.0.0.1:8080/api/asks"
        );
        assert_eq!(
            answer_url_from_api_base("http://127.0.0.1:8080/", "ask-9"),
            "http://127.0.0.1:8080/api/asks/ask-9/answer"
        );
    }
}
