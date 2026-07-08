use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};
use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use glance_catalog::leaf::Metric;
use glance_catalog::structural::{Cell, CellValue, ColumnSpec, Disclosure, Hero, Row, Table};
use glance_catalog::{Component, InlineNode, RenderContext, render_component};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    ApiError, ClipQueueItem, EvidenceLink, FeedKind, Glass, Post, Session, backlog_report,
    declared_summary, evidence_links_for_post, feed_kind_for_post, needs_you, rep1, review_report,
    sanctum_url, shell,
};

#[derive(Debug, Clone)]
pub(crate) struct NewReport {
    pub(crate) kind: String,
    pub(crate) scope_type: String,
    pub(crate) scope_value: Option<String>,
    pub(crate) window_start: Option<i64>,
    pub(crate) window_end: Option<i64>,
    pub(crate) title: String,
    pub(crate) doc_html: String,
    pub(crate) meta_json: Value,
    pub(crate) requested_by: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReportRecord {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) scope_type: String,
    pub(crate) scope_value: Option<String>,
    pub(crate) window_start: Option<i64>,
    pub(crate) window_end: Option<i64>,
    pub(crate) title: String,
    #[serde(skip_serializing)]
    pub(crate) doc_html: String,
    #[serde(rename = "meta")]
    pub(crate) meta_json: Value,
    pub(crate) generated_at: i64,
    pub(crate) requested_by: String,
}

impl ReportRecord {
    fn url(&self) -> String {
        format!("/reports/{}", self.id)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ActivityPost {
    pub(crate) post: Post,
    pub(crate) session: Session,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ReportsPageQuery {
    kind: Option<String>,
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GenerateReportRequest {
    kind: String,
    #[serde(default)]
    scope: Value,
    #[serde(default)]
    window: Value,
    #[serde(default, alias = "requestedBy")]
    requested_by: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ReportKind {
    ActivityDigest,
    FleetDigest,
    Backlog,
    ReviewIndex,
}

impl ReportKind {
    fn parse(raw: &str) -> Result<Self> {
        match raw.trim() {
            "activity-digest" | "activity" | "digest" => Ok(Self::ActivityDigest),
            "fleet-digest" | "fleet" | "rep1" => Ok(Self::FleetDigest),
            "backlog" => Ok(Self::Backlog),
            "review-index" | "review" | "reviews" => Ok(Self::ReviewIndex),
            other => bail!("unknown report kind: {other}"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::ActivityDigest => "activity-digest",
            Self::FleetDigest => "fleet-digest",
            Self::Backlog => "backlog",
            Self::ReviewIndex => "review-index",
        }
    }

    fn needs_window(self) -> bool {
        matches!(self, Self::ActivityDigest | Self::FleetDigest)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ReportScope {
    scope_type: String,
    scope_value: Option<String>,
}

impl ReportScope {
    fn fleet() -> Self {
        Self {
            scope_type: "fleet".to_string(),
            scope_value: None,
        }
    }

    fn parse(value: &Value) -> Result<Self> {
        match value {
            Value::Null => Ok(Self::fleet()),
            Value::String(raw) => Self::parse_string(raw),
            Value::Object(object) => {
                let scope_type = object
                    .get("type")
                    .or_else(|| object.get("scope_type"))
                    .or_else(|| object.get("scopeType"))
                    .and_then(Value::as_str)
                    .unwrap_or("fleet")
                    .trim();
                let scope_value = object
                    .get("value")
                    .or_else(|| object.get("scope_value"))
                    .or_else(|| object.get("scopeValue"))
                    .or_else(|| object.get("agent"))
                    .or_else(|| object.get("repo"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                Self::new(scope_type, scope_value)
            }
            _ => bail!("scope must be a string or object"),
        }
    }

    fn parse_string(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        if raw.is_empty() || raw == "fleet" {
            return Ok(Self::fleet());
        }
        if let Some((scope_type, scope_value)) = raw.split_once(':') {
            return Self::new(scope_type, Some(scope_value.trim().to_string()));
        }
        Self::new(raw, None)
    }

    fn new(scope_type: &str, scope_value: Option<String>) -> Result<Self> {
        let scope_type = scope_type.trim();
        match scope_type {
            "fleet" => Ok(Self::fleet()),
            "agent" | "repo" => {
                let Some(scope_value) = scope_value.filter(|value| !value.trim().is_empty()) else {
                    bail!("{scope_type} scope requires a value");
                };
                Ok(Self {
                    scope_type: scope_type.to_string(),
                    scope_value: Some(scope_value),
                })
            }
            other => bail!("unknown report scope: {other}"),
        }
    }

    fn label(&self) -> String {
        match (self.scope_type.as_str(), self.scope_value.as_deref()) {
            ("fleet", _) => "fleet".to_string(),
            ("agent", Some(agent)) => format!("agent {agent}"),
            ("repo", Some(repo)) => format!("repo {repo}"),
            _ => self.scope_type.clone(),
        }
    }

    fn synthesis_scope(&self) -> String {
        match (self.scope_type.as_str(), self.scope_value.as_deref()) {
            ("fleet", _) => "fleet".to_string(),
            ("agent", Some(agent)) => format!("agent:{agent}"),
            ("repo", Some(repo)) => format!("repo:{repo}"),
            _ => self.scope_type.clone(),
        }
    }

    fn matches_session(&self, session: &Session) -> bool {
        match self.scope_type.as_str() {
            "fleet" => true,
            "agent" => self
                .scope_value
                .as_deref()
                .is_some_and(|agent| session.agent == agent),
            "repo" => self.scope_value.as_deref().is_some_and(|repo| {
                session.cwd.as_deref().is_some_and(|cwd| {
                    cwd == repo
                        || cwd.ends_with(&format!("/{repo}"))
                        || cwd.contains(&format!("/{repo}/"))
                })
            }),
            _ => false,
        }
    }

    fn matches_card(&self, card: &PowderCard) -> bool {
        match self.scope_type.as_str() {
            "fleet" | "agent" => true,
            "repo" => {
                let Some(repo) = self.scope_value.as_deref() else {
                    return false;
                };
                card.repo
                    .as_deref()
                    .is_none_or(|card_repo| card_repo == repo)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedWindow {
    preset: String,
    start: i64,
    end: i64,
    label: String,
}

impl ResolvedWindow {
    fn since_rfc3339(&self) -> String {
        timestamp_rfc3339(self.start)
    }

    fn until_rfc3339(&self) -> String {
        timestamp_rfc3339(self.end)
    }
}

struct GeneratedDoc {
    title: String,
    doc_html: String,
    meta_json: Value,
}

#[derive(Debug, Clone, Deserialize)]
struct PowderCardsResponse {
    #[serde(default)]
    cards: Vec<PowderCard>,
}

#[derive(Debug, Clone, Deserialize)]
struct PowderCard {
    id: String,
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default, alias = "updatedAt")]
    updated_at: Option<i64>,
    #[serde(default, alias = "completedAt")]
    completed_at: Option<i64>,
}

struct PowderFetch {
    cards: Vec<PowderCard>,
    status: String,
}

struct SynthesisFetch {
    html: Option<String>,
    status: String,
}

#[derive(Default)]
struct AgentBucket {
    posts: Vec<ActivityPost>,
    clips: Vec<ClipQueueItem>,
    blocked: Vec<ActivityPost>,
}

pub(crate) async fn reports_shell(
    State(glass): State<Glass>,
    Query(query): Query<ReportsPageQuery>,
) -> Result<Html<String>, ApiError> {
    let reports = glass
        .list_reports()
        .map_err(|error| crate::api_failure("glass.reports.list.failed", "/reports", error))?;
    Ok(Html(shell::render_shell(shell::Shell {
        title: "Glass - Reports",
        active: Some(shell::Place::Reports),
        needs_you_count: needs_you::awaiting_input_count().await,
        sanctum_url: &sanctum_url(),
        styles: REPORTS_STYLE,
        body: &render_reports_body(&reports, &query),
        scripts: REPORTS_SCRIPT,
    })))
}

pub(crate) async fn report_doc_shell(
    State(glass): State<Glass>,
    AxumPath(id): AxumPath<String>,
) -> Result<Html<String>, ApiError> {
    let report = glass
        .get_report(&id)
        .map_err(|error| crate::api_failure("glass.reports.get.failed", "/reports/{id}", error))?;
    let body = render_report_doc_body(&report);
    Ok(Html(shell::render_shell(shell::Shell {
        title: &format!("Glass - {}", report.id),
        active: Some(shell::Place::Reports),
        needs_you_count: needs_you::awaiting_input_count().await,
        sanctum_url: &sanctum_url(),
        styles: REPORTS_STYLE,
        body: &body,
        scripts: "",
    })))
}

pub(crate) async fn list_reports(State(glass): State<Glass>) -> Result<Json<Value>, ApiError> {
    let reports = glass
        .list_reports()
        .map_err(|error| crate::api_failure("glass.reports.list.failed", "/api/reports", error))?;
    Ok(Json(json!({
        "reports": reports.iter().map(report_summary).collect::<Vec<_>>()
    })))
}

pub(crate) async fn post_report(
    State(glass): State<Glass>,
    Json(input): Json<GenerateReportRequest>,
) -> Result<Json<Value>, ApiError> {
    let report = generate_and_persist(&glass, input).await.map_err(|error| {
        crate::api_failure("glass.reports.generate.failed", "/api/reports", error)
    })?;
    Ok(Json(json!({ "id": report.id, "url": report.url() })))
}

pub(crate) async fn redirect_rep1() -> Response {
    redirect_301("/reports?kind=fleet-digest")
}

pub(crate) async fn redirect_backlog(AxumPath(repo): AxumPath<String>) -> Response {
    redirect_301(&format!(
        "/reports?kind=backlog&scope=repo%3A{}",
        url_encode_component(&repo)
    ))
}

async fn generate_and_persist(glass: &Glass, input: GenerateReportRequest) -> Result<ReportRecord> {
    let kind = ReportKind::parse(&input.kind)?;
    let scope = ReportScope::parse(&input.scope)?;
    let window = if kind.needs_window() {
        Some(resolve_window(&input.window)?)
    } else {
        None
    };
    let requested_by = input
        .requested_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("you")
        .to_string();

    let generated = match kind {
        ReportKind::ActivityDigest => {
            generate_activity_digest(glass, &scope, window.as_ref().expect("window")).await?
        }
        ReportKind::FleetDigest => generate_fleet_digest(window.as_ref().expect("window")).await?,
        ReportKind::Backlog => generate_backlog(&scope).await?,
        ReportKind::ReviewIndex => generate_review_index(glass)?,
    };

    glass.create_report(NewReport {
        kind: kind.as_str().to_string(),
        scope_type: scope.scope_type,
        scope_value: scope.scope_value,
        window_start: window.as_ref().map(|window| window.start),
        window_end: window.as_ref().map(|window| window.end),
        title: generated.title,
        doc_html: generated.doc_html,
        meta_json: generated.meta_json,
        requested_by,
    })
}

fn report_summary(report: &ReportRecord) -> Value {
    json!({
        "id": report.id,
        "url": report.url(),
        "kind": report.kind,
        "scopeType": report.scope_type,
        "scopeValue": report.scope_value,
        "scope": scope_label(&report.scope_type, report.scope_value.as_deref()),
        "windowStart": report.window_start,
        "windowEnd": report.window_end,
        "window": format_report_window(report.window_start, report.window_end),
        "title": report.title,
        "generatedAt": report.generated_at,
        "requestedBy": report.requested_by,
        "meta": report.meta_json,
    })
}

fn render_reports_body(reports: &[ReportRecord], query: &ReportsPageQuery) -> String {
    let initial_kind = html_escape(query.kind.as_deref().unwrap_or("activity-digest"));
    let initial_scope = html_escape(query.scope.as_deref().unwrap_or("fleet"));
    format!(
        r#"<div class="reports-shell" data-initial-kind="{initial_kind}" data-initial-scope="{initial_scope}">
  <section class="reports-generator">
    <p class="ae-plate-cap">GENERATE A REPORT</p>
    <div class="reports-gen-row">
      <span class="ae-h">WINDOW</span>
      <span class="reports-chipset" data-report-group="window">
        <button type="button" class="reports-chip" data-window="today">Today</button>
        <button type="button" class="reports-chip" data-window="yesterday">Yesterday</button>
        <button type="button" class="reports-chip" data-window="this-week">This week</button>
        <button type="button" class="reports-chip is-on" data-window="last-week">Last week</button>
        <button type="button" class="reports-chip" data-window="custom">Custom</button>
      </span>
      <code class="reports-range" id="reports-range"></code>
    </div>
    <div class="reports-custom" id="reports-custom">
      <label>Start <input class="ae-input" id="reports-start" type="date"></label>
      <label>End <input class="ae-input" id="reports-end" type="date"></label>
    </div>
    <div class="reports-gen-row">
      <span class="ae-h">SCOPE</span>
      <span class="reports-chipset" data-report-group="scope">
        <button type="button" class="reports-chip is-on" data-scope="fleet">Whole fleet</button>
        <button type="button" class="reports-chip" data-scope="agent">One agent</button>
        <button type="button" class="reports-chip" data-scope="repo">One repo</button>
      </span>
      <input class="ae-input reports-scope-value" id="reports-scope-value" placeholder="agent or repo" aria-label="scope value">
    </div>
    <div class="reports-gen-row">
      <span class="ae-h">KIND</span>
      <span class="reports-chipset" data-report-group="kind">
        <button type="button" class="reports-chip is-on" data-kind="activity-digest">Activity digest</button>
        <button type="button" class="reports-chip" data-kind="backlog">Backlog</button>
        <button type="button" class="reports-chip" data-kind="review-index">Review index</button>
        <button type="button" class="reports-chip" data-kind="fleet-digest">Fleet digest</button>
      </span>
      <span></span>
    </div>
    <button class="ae-button" id="reports-generate" type="button">Generate report</button>
    <span class="reports-status" id="reports-status" role="status"></span>
  </section>

  <section class="ae-plate reports-library">
    <p class="ae-plate-cap">PLATE 1 - THE LIBRARY - EVERY GENERATED REPORT, NEWEST FIRST</p>
    <div class="reports-table-scroll">
      <table class="ae-table">
        <thead><tr><th>ID</th><th>REPORT</th><th>WINDOW</th><th>SCOPE</th><th>GENERATED</th></tr></thead>
        <tbody>{rows}</tbody>
      </table>
    </div>
  </section>
</div>"#,
        rows = render_library_rows(reports),
    )
}

fn render_library_rows(reports: &[ReportRecord]) -> String {
    if reports.is_empty() {
        return r#"<tr><td data-label="ID" colspan="5">No generated reports yet.</td></tr>"#
            .to_string();
    }
    reports
        .iter()
        .map(|report| {
            format!(
                r#"<tr>
  <td data-label="ID">{id}</td>
  <td data-label="REPORT"><a href="{url}">{title}</a></td>
  <td data-label="WINDOW">{window}</td>
  <td data-label="SCOPE">{scope}</td>
  <td data-label="GENERATED">{generated} - {requested_by}</td>
</tr>"#,
                id = html_escape(&report.id),
                url = html_escape(&report.url()),
                title = html_escape(&report.title),
                window = html_escape(&format_report_window(
                    report.window_start,
                    report.window_end
                )),
                scope = html_escape(&scope_label(
                    &report.scope_type,
                    report.scope_value.as_deref()
                )),
                generated = html_escape(&format_timestamp(report.generated_at)),
                requested_by = html_escape(&report.requested_by),
            )
        })
        .collect()
}

fn render_report_doc_body(report: &ReportRecord) -> String {
    format!(
        r#"<article class="ae-doc reports-doc">
  <header class="reports-doc-head">
    <p class="ae-plate-cap">{id}</p>
    <h1>{title}</h1>
    <dl>
      <div><dt>WINDOW</dt><dd>{window}</dd></div>
      <div><dt>SCOPE</dt><dd>{scope}</dd></div>
      <div><dt>GENERATED</dt><dd>{generated} - {requested_by}</dd></div>
    </dl>
  </header>
  <div class="reports-doc-body">{doc_html}</div>
</article>"#,
        id = html_escape(&report.id),
        title = html_escape(&report.title),
        window = html_escape(&format_report_window(
            report.window_start,
            report.window_end
        )),
        scope = html_escape(&scope_label(
            &report.scope_type,
            report.scope_value.as_deref()
        )),
        generated = html_escape(&format_timestamp(report.generated_at)),
        requested_by = html_escape(&report.requested_by),
        doc_html = report.doc_html,
    )
}

async fn generate_activity_digest(
    glass: &Glass,
    scope: &ReportScope,
    window: &ResolvedWindow,
) -> Result<GeneratedDoc> {
    let posts = glass
        .list_activity_posts(window.start, window.end)?
        .into_iter()
        .filter(|item| item.session.agent != "glass-doctor")
        .filter(|item| scope.matches_session(&item.session))
        .collect::<Vec<_>>();
    let clips = glass
        .list_activity_clips(window.start, window.end)?
        .into_iter()
        .filter(|item| item.context.session.agent != "glass-doctor")
        .filter(|item| scope.matches_session(&item.context.session))
        .collect::<Vec<_>>();
    let powder = fetch_completed_powder_cards(scope, window).await;
    let synthesis = fetch_synthesis_html(scope, window).await;
    let blocked_count = posts
        .iter()
        .filter(|item| feed_kind_for_post(&item.post) == FeedKind::Blocked)
        .count();

    let mut html = render_component_list(&[activity_hero(
        scope,
        window,
        powder.cards.len(),
        posts.len(),
        clips.len(),
        blocked_count,
    )])?;
    if let Some(synthesis_html) = synthesis.html.as_deref() {
        html.push_str("<section class=\"reports-synthesis\"><p class=\"ae-plate-cap\">SYNTHESIS NARRATIVE</p>");
        html.push_str(synthesis_html);
        html.push_str("</section>");
    }
    let mut sections = vec![powder_completion_table(&powder.cards)];
    sections.extend(agent_activity_sections(posts.clone(), clips.clone()));
    html.push_str(&render_component_list(&sections)?);

    Ok(GeneratedDoc {
        title: format!("Activity digest - {}", scope.label()),
        doc_html: html,
        meta_json: json!({
            "window": {
                "preset": &window.preset,
                "start": window.start,
                "end": window.end,
                "since": window.since_rfc3339(),
                "until": window.until_rfc3339(),
                "label": &window.label,
            },
            "scope": {
                "type": &scope.scope_type,
                "value": &scope.scope_value,
            },
            "counts": {
                "powderCompleted": powder.cards.len(),
                "glassPosts": posts.len(),
                "clips": clips.len(),
                "blocked": blocked_count,
            },
            "powderStatus": powder.status,
            "synthesisStatus": synthesis.status,
        }),
    })
}

async fn generate_fleet_digest(window: &ResolvedWindow) -> Result<GeneratedDoc> {
    let tab = if window.end - window.start > 2 * 86_400 {
        "7d"
    } else {
        "24h"
    };
    let (doc_html, generated_at) = rep1::generate_rep1_html(tab)
        .await
        .map_err(|err| anyhow!(err))?;
    Ok(GeneratedDoc {
        title: format!("Fleet digest - {}", window.label),
        doc_html,
        meta_json: json!({
            "rep1Window": tab,
            "sourceGeneratedAt": generated_at,
            "window": {
                "preset": &window.preset,
                "start": window.start,
                "end": window.end,
                "since": window.since_rfc3339(),
                "until": window.until_rfc3339(),
                "label": &window.label,
            },
        }),
    })
}

async fn generate_backlog(scope: &ReportScope) -> Result<GeneratedDoc> {
    if scope.scope_type != "repo" {
        bail!("backlog reports require repo scope");
    }
    let repo = scope
        .scope_value
        .as_deref()
        .ok_or_else(|| anyhow!("backlog reports require repo scope"))?;
    let (doc_html, count) = backlog_report::generate_backlog_html(repo)
        .await
        .map_err(|err| anyhow!(err))?;
    Ok(GeneratedDoc {
        title: format!("Backlog - {repo}"),
        doc_html,
        meta_json: json!({ "repo": repo, "cardCount": count }),
    })
}

fn generate_review_index(glass: &Glass) -> Result<GeneratedDoc> {
    let reports = glass.list_reports()?;
    let review_rows = reports
        .iter()
        .filter(|report| report.kind.contains("review"))
        .collect::<Vec<_>>();
    let index_html = render_component_list(&[
        Component::Hero(Hero {
            title: "Review index".to_string(),
            summary: text("Persisted review surfaces and the current narrated review sample."),
            stats: vec![Metric {
                label: "Persisted reviews".to_string(),
                value: review_rows.len().to_string(),
            }],
            image_intent: None,
        }),
        Component::Table(review_index_table(&review_rows)),
    ])?;
    let (sample_title, sample_html) =
        review_report::generate_sample_review_html().map_err(|err| anyhow!(err))?;
    Ok(GeneratedDoc {
        title: "Review index".to_string(),
        doc_html: format!("{index_html}{sample_html}"),
        meta_json: json!({
            "reviewCount": review_rows.len(),
            "sample": sample_title,
        }),
    })
}

fn activity_hero(
    scope: &ReportScope,
    window: &ResolvedWindow,
    powder_completed: usize,
    posts: usize,
    clips: usize,
    blocked: usize,
) -> Component {
    Component::Hero(Hero {
        title: format!("Activity digest - {}", scope.label()),
        summary: text(format!(
            "{}: {powder_completed} completed Powder card(s), {posts} Glass post(s), {clips} clip(s), {blocked} blocked event(s).",
            window.label
        )),
        stats: vec![
            metric("Completed cards", powder_completed),
            metric("Glass posts", posts),
            metric("Clips", clips),
            metric("Blocked", blocked),
        ],
        image_intent: None,
    })
}

fn powder_completion_table(cards: &[PowderCard]) -> Component {
    Component::Table(Table {
        heading: "Powder completions".to_string(),
        columns: vec![
            column("card", "Card", false, true),
            column("title", "Title", false, false),
            column("repo", "Repo", false, false),
            column("priority", "Priority", false, false),
            column("completed", "Completed", false, false),
            column("evidence", "Evidence", false, false),
        ],
        rows: cards
            .iter()
            .map(|card| Row {
                cells: vec![
                    text_cell("card", &card.id),
                    text_cell("title", &card.title),
                    text_cell("repo", card.repo.as_deref().unwrap_or("-")),
                    text_cell("priority", card.priority.as_deref().unwrap_or("-")),
                    text_cell("completed", format_timestamp(card.activity_timestamp())),
                    link_cell("evidence", "card", powder_card_url(&card.id)),
                ],
            })
            .collect(),
        empty_note: cards
            .is_empty()
            .then(|| "No Powder completions found for this window and scope.".to_string()),
        demoted_note: None,
    })
}

fn agent_activity_sections(posts: Vec<ActivityPost>, clips: Vec<ClipQueueItem>) -> Vec<Component> {
    let mut by_agent = BTreeMap::<String, AgentBucket>::new();
    for item in posts {
        let bucket = by_agent.entry(item.session.agent.clone()).or_default();
        if feed_kind_for_post(&item.post) == FeedKind::Blocked {
            bucket.blocked.push(item.clone());
        }
        bucket.posts.push(item);
    }
    for item in clips {
        by_agent
            .entry(item.context.session.agent.clone())
            .or_default()
            .clips
            .push(item);
    }

    if by_agent.is_empty() {
        return vec![Component::Table(Table {
            heading: "Agent activity".to_string(),
            columns: vec![column("note", "Note", false, true)],
            rows: vec![],
            empty_note: Some(
                "No Glass posts or clips found for this window and scope.".to_string(),
            ),
            demoted_note: None,
        })];
    }

    by_agent
        .into_iter()
        .map(|(agent, bucket)| {
            let mut children = vec![
                posts_table(&bucket.posts),
                clips_table(&bucket.clips),
                blocked_table(&bucket.blocked),
            ];
            children.retain(|component| !table_is_empty(component));
            Component::Disclosure(Disclosure {
                heading: format!("Agent: {agent}"),
                children,
            })
        })
        .collect()
}

fn posts_table(posts: &[ActivityPost]) -> Component {
    Component::Table(Table {
        heading: "Posts".to_string(),
        columns: vec![
            column("at", "At", false, false),
            column("kind", "Kind", false, false),
            column("title", "Title", false, true),
            column("summary", "Summary", false, false),
            column("evidence", "Evidence", false, false),
        ],
        rows: posts
            .iter()
            .map(|item| {
                let links = evidence_links_for_post(&item.post);
                Row {
                    cells: vec![
                        text_cell(
                            "at",
                            format_timestamp(item.post.updated_at.max(item.post.created_at)),
                        ),
                        text_cell("kind", feed_kind_for_post(&item.post).as_str()),
                        text_cell("title", &item.post.title),
                        text_cell(
                            "summary",
                            declared_summary(&item.post).unwrap_or_else(|| {
                                format!("{} surface(s)", item.post.surfaces.len())
                            }),
                        ),
                        first_link_cell("evidence", links, post_url(&item.post)),
                    ],
                }
            })
            .collect(),
        empty_note: Some("No posts in this window.".to_string()),
        demoted_note: None,
    })
}

fn clips_table(clips: &[ClipQueueItem]) -> Component {
    Component::Table(Table {
        heading: "Clips".to_string(),
        columns: vec![
            column("at", "At", false, false),
            column("caption", "Caption", false, true),
            column("surface", "Surface", false, false),
            column("evidence", "Evidence", false, false),
        ],
        rows: clips
            .iter()
            .map(|item| {
                let surface = item
                    .context
                    .surface
                    .as_ref()
                    .map(|surface| format!("{} {}", surface.kind, surface.id))
                    .unwrap_or_else(|| "whole post".to_string());
                Row {
                    cells: vec![
                        text_cell("at", format_timestamp(item.clip.created_at)),
                        text_cell("caption", &item.draft_caption),
                        text_cell("surface", surface),
                        first_link_cell(
                            "evidence",
                            item.context
                                .evidence_links
                                .iter()
                                .map(|link| EvidenceLink {
                                    label: link.label.clone(),
                                    url: link.url.clone(),
                                })
                                .collect(),
                            post_url(&item.context.post),
                        ),
                    ],
                }
            })
            .collect(),
        empty_note: Some("No clips in this window.".to_string()),
        demoted_note: None,
    })
}

fn blocked_table(posts: &[ActivityPost]) -> Component {
    Component::Table(Table {
        heading: "Blocked events".to_string(),
        columns: vec![
            column("at", "At", false, false),
            column("title", "Title", false, true),
            column("summary", "Summary", false, false),
            column("evidence", "Evidence", false, false),
        ],
        rows: posts
            .iter()
            .map(|item| Row {
                cells: vec![
                    text_cell(
                        "at",
                        format_timestamp(item.post.updated_at.max(item.post.created_at)),
                    ),
                    text_cell("title", &item.post.title),
                    text_cell(
                        "summary",
                        declared_summary(&item.post).unwrap_or_else(|| "blocked".to_string()),
                    ),
                    first_link_cell(
                        "evidence",
                        evidence_links_for_post(&item.post),
                        post_url(&item.post),
                    ),
                ],
            })
            .collect(),
        empty_note: Some("No blocked events in this window.".to_string()),
        demoted_note: None,
    })
}

fn review_index_table(reports: &[&ReportRecord]) -> Table {
    Table {
        heading: "Persisted review reports".to_string(),
        columns: vec![
            column("id", "ID", false, true),
            column("title", "Title", false, false),
            column("generated", "Generated", false, false),
            column("url", "URL", false, false),
        ],
        rows: reports
            .iter()
            .map(|report| Row {
                cells: vec![
                    text_cell("id", &report.id),
                    text_cell("title", &report.title),
                    text_cell("generated", format_timestamp(report.generated_at)),
                    link_cell("url", report.url(), report.url()),
                ],
            })
            .collect(),
        empty_note: Some("No persisted review reports yet.".to_string()),
        demoted_note: None,
    }
}

async fn fetch_completed_powder_cards(scope: &ReportScope, window: &ResolvedWindow) -> PowderFetch {
    let Some(base) = std::env::var("GLASS_POWDER_API_BASE_URL")
        .ok()
        .filter(|value| !value.is_empty())
    else {
        return PowderFetch {
            cards: Vec::new(),
            status: "missing GLASS_POWDER_API_BASE_URL".to_string(),
        };
    };
    let Some(key) = std::env::var("GLASS_POWDER_API_KEY")
        .ok()
        .filter(|value| !value.is_empty())
    else {
        return PowderFetch {
            cards: Vec::new(),
            status: "missing GLASS_POWDER_API_KEY".to_string(),
        };
    };
    let mut url = format!("{}/api/v1/cards?limit=500", base.trim_end_matches('/'));
    if scope.scope_type == "repo"
        && let Some(repo) = scope.scope_value.as_deref()
    {
        url.push_str("&repo=");
        url.push_str(&url_encode_component(repo));
    }
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            return PowderFetch {
                cards: Vec::new(),
                status: format!("client error: {err}"),
            };
        }
    };
    let response = match client.get(&url).bearer_auth(key).send().await {
        Ok(response) => response,
        Err(err) => {
            return PowderFetch {
                cards: Vec::new(),
                status: format!("transport error: {err}"),
            };
        }
    };
    if !response.status().is_success() {
        return PowderFetch {
            cards: Vec::new(),
            status: format!("upstream returned {}", response.status()),
        };
    }
    let body = match response.json::<PowderCardsResponse>().await {
        Ok(body) => body,
        Err(err) => {
            return PowderFetch {
                cards: Vec::new(),
                status: format!("parse error: {err}"),
            };
        }
    };
    let mut cards = body
        .cards
        .into_iter()
        .filter(|card| card.status == "done" || card.completed_at.is_some())
        .filter(|card| {
            let ts = card.activity_timestamp();
            ts >= window.start && ts < window.end
        })
        .filter(|card| scope.matches_card(card))
        .collect::<Vec<_>>();
    cards.sort_by(|left, right| {
        right
            .activity_timestamp()
            .cmp(&left.activity_timestamp())
            .then_with(|| left.id.cmp(&right.id))
    });
    PowderFetch {
        status: format!("ok: {} completed card(s)", cards.len()),
        cards,
    }
}

async fn fetch_synthesis_html(scope: &ReportScope, window: &ResolvedWindow) -> SynthesisFetch {
    let Some(endpoint) = std::env::var("GLASS_SYNTHESIS_ENDPOINT")
        .ok()
        .filter(|value| !value.is_empty())
    else {
        return SynthesisFetch {
            html: None,
            status: "not configured".to_string(),
        };
    };
    let request = json!({
        "window": if window.preset == "custom" { "custom" } else { window.preset.as_str() },
        "since": window.since_rfc3339(),
        "until": window.until_rfc3339(),
        "scope": scope.synthesis_scope(),
    });
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            return SynthesisFetch {
                html: None,
                status: format!("client error: {err}"),
            };
        }
    };
    let response = match client.post(&endpoint).json(&request).send().await {
        Ok(response) => response,
        Err(err) => {
            return SynthesisFetch {
                html: None,
                status: format!("transport error: {err}"),
            };
        }
    };
    if !response.status().is_success() {
        return SynthesisFetch {
            html: None,
            status: format!("upstream returned {}", response.status()),
        };
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let value = if content_type.contains("text/event-stream") {
        match response.text().await {
            Ok(text) => parse_final_sse_spec(&text),
            Err(err) => Err(format!("read synthesis stream: {err}")),
        }
    } else {
        match response.json::<Value>().await {
            Ok(value) => Ok(spec_from_synthesis_value(value)),
            Err(err) => Err(format!("parse synthesis response: {err}")),
        }
    };
    let spec = match value {
        Ok(Some(spec)) => spec,
        Ok(None) => {
            return SynthesisFetch {
                html: None,
                status: "no full spec in synthesis response".to_string(),
            };
        }
        Err(err) => {
            return SynthesisFetch {
                html: None,
                status: err,
            };
        }
    };
    match rep1::render_spec_value(spec) {
        Ok((html, _)) => SynthesisFetch {
            html: Some(html),
            status: "ok".to_string(),
        },
        Err(err) => SynthesisFetch {
            html: None,
            status: format!("render error: {err}"),
        },
    }
}

fn resolve_window(value: &Value) -> Result<ResolvedWindow> {
    match value {
        Value::Null => resolve_preset("last-week"),
        Value::String(preset) => resolve_preset(preset),
        Value::Object(object) => {
            let preset = object
                .get("preset")
                .or_else(|| object.get("type"))
                .or_else(|| object.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("last-week");
            if preset == "custom" {
                let start = object
                    .get("start")
                    .or_else(|| object.get("since"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("custom windows require start"))?;
                let end = object
                    .get("end")
                    .or_else(|| object.get("until"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("custom windows require end"))?;
                resolve_custom_window(start, end)
            } else {
                resolve_preset(preset)
            }
        }
        _ => bail!("window must be a preset string or object"),
    }
}

fn resolve_preset(preset: &str) -> Result<ResolvedWindow> {
    let today = Local::now().date_naive();
    let week_start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    let (start_date, end_date) = match preset {
        "today" => (today, today + Duration::days(1)),
        "yesterday" => (today - Duration::days(1), today),
        "this-week" => (week_start, week_start + Duration::days(7)),
        "last-week" => (week_start - Duration::days(7), week_start),
        other => bail!("unknown window preset: {other}"),
    };
    build_window(preset, start_date, end_date)
}

fn resolve_custom_window(start: &str, end: &str) -> Result<ResolvedWindow> {
    let start_date = NaiveDate::parse_from_str(start, "%Y-%m-%d")
        .map_err(|err| anyhow!("custom start must be YYYY-MM-DD: {err}"))?;
    let end_date = NaiveDate::parse_from_str(end, "%Y-%m-%d")
        .map_err(|err| anyhow!("custom end must be YYYY-MM-DD: {err}"))?;
    build_window("custom", start_date, end_date)
}

fn build_window(
    preset: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<ResolvedWindow> {
    let start = local_midnight_timestamp(start_date)?;
    let end = local_midnight_timestamp(end_date)?;
    if start >= end {
        bail!("window start must be before end");
    }
    Ok(ResolvedWindow {
        preset: preset.to_string(),
        start,
        end,
        label: format!(
            "{} - {}",
            start_date.format("%Y-%m-%d"),
            end_date.format("%Y-%m-%d")
        ),
    })
}

fn local_midnight_timestamp(date: NaiveDate) -> Result<i64> {
    let naive = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow!("invalid local midnight"))?;
    let local = Local
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&naive).latest())
        .ok_or_else(|| anyhow!("could not resolve local midnight for {date}"))?;
    Ok(local.with_timezone(&Utc).timestamp())
}

fn render_component_list(components: &[Component]) -> Result<String> {
    let ctx = RenderContext {
        now: Utc::now(),
        cite_href: &|ref_id| format!("#cite-{ref_id}"),
    };
    let mut html = String::new();
    for component in components {
        component
            .validate()
            .map_err(|err| anyhow!("invalid report component: {err}"))?;
        html.push_str(&render_component(component, &ctx));
    }
    Ok(html)
}

fn parse_final_sse_spec(text: &str) -> Result<Option<Value>, String> {
    let mut event = "message".to_string();
    let mut data = Vec::<String>::new();
    let mut full = None;
    for line in text.replace("\r\n", "\n").replace('\r', "\n").lines() {
        if line.trim().is_empty() {
            if event == "full" && !data.is_empty() {
                let value = serde_json::from_str::<Value>(&data.join("\n"))
                    .map_err(|err| format!("parse full synthesis event: {err}"))?;
                full = spec_from_synthesis_value(value);
            }
            event = "message".to_string();
            data.clear();
            continue;
        }
        if line.starts_with(':') {
            continue;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" if !value.trim().is_empty() => event = value.trim().to_string(),
            "data" => data.push(value.to_string()),
            _ => {}
        }
    }
    if event == "full" && !data.is_empty() {
        let value = serde_json::from_str::<Value>(&data.join("\n"))
            .map_err(|err| format!("parse full synthesis event: {err}"))?;
        full = spec_from_synthesis_value(value);
    }
    Ok(full)
}

fn spec_from_synthesis_value(value: Value) -> Option<Value> {
    value.get("spec").cloned().or_else(|| {
        if value.get("stage").is_some() {
            None
        } else {
            Some(value)
        }
    })
}

fn table_is_empty(component: &Component) -> bool {
    matches!(component, Component::Table(table) if table.rows.is_empty())
}

impl PowderCard {
    fn activity_timestamp(&self) -> i64 {
        self.completed_at.or(self.updated_at).unwrap_or_default()
    }
}

fn metric(label: &str, value: usize) -> Metric {
    Metric {
        label: label.to_string(),
        value: value.to_string(),
    }
}

fn column(key: &str, label: &str, numeric: bool, emphasize: bool) -> ColumnSpec {
    ColumnSpec {
        key: key.to_string(),
        label: label.to_string(),
        numeric,
        emphasize,
    }
}

fn text_cell(key: &str, text: impl Into<String>) -> Cell {
    Cell {
        column_key: key.to_string(),
        value: CellValue::Text { text: text.into() },
    }
}

fn link_cell(key: &str, text: impl Into<String>, href: impl Into<String>) -> Cell {
    Cell {
        column_key: key.to_string(),
        value: CellValue::Link {
            text: text.into(),
            href: href.into(),
        },
    }
}

fn first_link_cell(key: &str, links: Vec<EvidenceLink>, fallback: String) -> Cell {
    let link = links.into_iter().next().unwrap_or(EvidenceLink {
        label: "post".to_string(),
        url: fallback,
    });
    link_cell(key, link.label, link.url)
}

fn text(s: impl Into<String>) -> Vec<InlineNode> {
    vec![InlineNode::Text { text: s.into() }]
}

fn post_url(post: &Post) -> String {
    format!("/session/{}/p/{}", post.session_id, post.id)
}

fn powder_card_url(card_id: &str) -> String {
    let base = std::env::var("GLASS_POWDER_BOARD_URL")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("GLASS_POWDER_API_BASE_URL")
                .ok()
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "/".to_string());
    let trimmed = base.trim_end_matches('/');
    let board = if trimmed.ends_with("/board") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/board")
    };
    format!("{board}#card-{}", url_encode_component(card_id))
}

fn scope_label(scope_type: &str, scope_value: Option<&str>) -> String {
    match (scope_type, scope_value) {
        ("fleet", _) => "fleet".to_string(),
        ("agent", Some(value)) => format!("agent {value}"),
        ("repo", Some(value)) => format!("repo {value}"),
        _ => scope_type.to_string(),
    }
}

fn format_report_window(start: Option<i64>, end: Option<i64>) -> String {
    match start.zip(end) {
        Some((start, end)) => format!("{} - {}", format_local_date(start), format_local_date(end)),
        None => "-".to_string(),
    }
}

fn format_local_date(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|dt| {
            dt.with_timezone(&Local)
                .date_naive()
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|| ts.to_string())
}

fn format_timestamp(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| ts.to_string())
}

fn timestamp_rfc3339(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("unix epoch"))
        .to_rfc3339()
}

fn redirect_301(location: &str) -> Response {
    (
        StatusCode::MOVED_PERMANENTLY,
        [(header::LOCATION, location.to_string())],
    )
        .into_response()
}

fn url_encode_component(raw: &str) -> String {
    let mut out = String::new();
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
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

const REPORTS_STYLE: &str = r#"
.reports-shell { max-width: 1040px; margin: 0 auto; padding: var(--ae-space-6) var(--ae-space-5); }
.reports-generator { margin-bottom: var(--ae-space-6); border-bottom: 1px solid var(--ae-line); padding-bottom: var(--ae-space-6); }
.reports-gen-row { display: grid; grid-template-columns: 6.5rem minmax(0, 1fr) minmax(12rem, auto); gap: var(--ae-space-3); align-items: center; margin-top: var(--ae-space-4); }
.reports-chipset { display: flex; flex-wrap: wrap; gap: var(--ae-space-2); }
.reports-chip { border: 1px solid var(--ae-line); background: var(--ae-surface); color: var(--ae-ink); padding: 0.55rem 0.8rem; font: inherit; cursor: pointer; }
.reports-chip.is-on { background: var(--ae-ink); border-color: var(--ae-ink); color: var(--ae-surface); }
.reports-range { justify-self: end; color: var(--ae-ink-muted); white-space: nowrap; }
.reports-custom { display: none; grid-template-columns: repeat(2, minmax(12rem, 1fr)); gap: var(--ae-space-3); margin-top: var(--ae-space-3); margin-left: 6.5rem; }
.reports-custom.is-on { display: grid; }
.reports-custom label { display: grid; gap: 0.35rem; color: var(--ae-ink-muted); font-size: 13px; }
.reports-scope-value { display: none; width: 100%; min-width: 12rem; }
.reports-scope-value.is-on { display: block; }
.reports-status { margin-left: var(--ae-space-3); color: var(--ae-ink-muted); font-size: 13px; }
.reports-library { overflow: hidden; }
.reports-table-scroll { overflow-x: auto; }
.reports-doc { max-width: 980px; margin: 0 auto; padding: var(--ae-space-6) var(--ae-space-5); }
.reports-doc-head { border-bottom: 1px solid var(--ae-line); margin-bottom: var(--ae-space-5); padding-bottom: var(--ae-space-5); }
.reports-doc-head h1 { font-size: clamp(1.5rem, 2.5vw, 2.4rem); line-height: 1.05; margin: 0.2rem 0 var(--ae-space-4); }
.reports-doc-head dl { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: var(--ae-space-3); margin: 0; }
.reports-doc-head dt { color: var(--ae-ink-muted); font-size: 11px; letter-spacing: 0; }
.reports-doc-head dd { margin: 0.2rem 0 0; font-family: var(--ae-font-mono); font-size: 13px; }
.reports-doc-body { display: grid; gap: var(--ae-space-5); }
.reports-synthesis { display: grid; gap: var(--ae-space-3); padding: var(--ae-space-4); border: 1px solid var(--ae-line); }
@media (max-width: 720px) {
  .reports-gen-row { grid-template-columns: 1fr; }
  .reports-range { justify-self: start; }
  .reports-custom { margin-left: 0; grid-template-columns: 1fr; }
  .reports-doc-head dl { grid-template-columns: 1fr; }
}
"#;

const REPORTS_SCRIPT: &str = r#"
(function(){
  var root = document.querySelector('.reports-shell');
  if (!root) return;
  var state = {
    window: 'last-week',
    scope: 'fleet',
    scopeValue: '',
    kind: root.dataset.initialKind || 'activity-digest'
  };
  var initialScope = root.dataset.initialScope || 'fleet';
  if (initialScope.indexOf(':') > -1) {
    var parts = initialScope.split(':');
    state.scope = parts[0];
    state.scopeValue = parts.slice(1).join(':');
  }
  var rangeEl = document.getElementById('reports-range');
  var customEl = document.getElementById('reports-custom');
  var startEl = document.getElementById('reports-start');
  var endEl = document.getElementById('reports-end');
  var scopeValueEl = document.getElementById('reports-scope-value');
  var statusEl = document.getElementById('reports-status');
  var generateEl = document.getElementById('reports-generate');

  function pad(n) { return String(n).padStart(2, '0'); }
  function isoDate(d) { return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()); }
  function addDays(d, n) { var x = new Date(d.getFullYear(), d.getMonth(), d.getDate()); x.setDate(x.getDate() + n); return x; }
  function monday(d) {
    var x = new Date(d.getFullYear(), d.getMonth(), d.getDate());
    var day = (x.getDay() + 6) % 7;
    x.setDate(x.getDate() - day);
    return x;
  }
  function rangeForPreset(name) {
    var today = new Date();
    today = new Date(today.getFullYear(), today.getMonth(), today.getDate());
    var week = monday(today);
    if (name === 'today') return [today, addDays(today, 1)];
    if (name === 'yesterday') return [addDays(today, -1), today];
    if (name === 'this-week') return [week, addDays(week, 7)];
    if (name === 'last-week') return [addDays(week, -7), week];
    return [startEl.valueAsDate || today, endEl.valueAsDate || addDays(today, 1)];
  }
  function setActive(group, attr, value) {
    document.querySelectorAll('[data-report-group="' + group + '"] .reports-chip').forEach(function(btn){
      btn.classList.toggle('is-on', btn.getAttribute(attr) === value);
    });
  }
  function sync() {
    if (state.kind === 'backlog' && state.scope !== 'repo') {
      state.scope = 'repo';
      if (!state.scopeValue) state.scopeValue = 'glass';
    }
    setActive('window', 'data-window', state.window);
    setActive('scope', 'data-scope', state.scope);
    setActive('kind', 'data-kind', state.kind);
    customEl.classList.toggle('is-on', state.window === 'custom');
    scopeValueEl.classList.toggle('is-on', state.scope !== 'fleet');
    scopeValueEl.placeholder = state.scope === 'agent' ? 'agent name' : 'repo name';
    if (state.scope !== 'fleet') scopeValueEl.value = state.scopeValue;
    var range = rangeForPreset(state.window);
    rangeEl.textContent = isoDate(range[0]) + ' -> ' + isoDate(range[1]);
    if (!startEl.value) startEl.value = isoDate(range[0]);
    if (!endEl.value) endEl.value = isoDate(range[1]);
  }
  document.querySelectorAll('[data-window]').forEach(function(btn){
    btn.addEventListener('click', function(){ state.window = btn.dataset.window; sync(); });
  });
  document.querySelectorAll('[data-scope]').forEach(function(btn){
    btn.addEventListener('click', function(){ state.scope = btn.dataset.scope; sync(); });
  });
  document.querySelectorAll('[data-kind]').forEach(function(btn){
    btn.addEventListener('click', function(){ state.kind = btn.dataset.kind; sync(); });
  });
  scopeValueEl.addEventListener('input', function(){ state.scopeValue = scopeValueEl.value.trim(); });
  startEl.addEventListener('input', sync);
  endEl.addEventListener('input', sync);
  generateEl.addEventListener('click', async function(){
    statusEl.textContent = 'Generating...';
    generateEl.disabled = true;
    state.scopeValue = scopeValueEl.value.trim();
    var payload = {
      kind: state.kind,
      requestedBy: 'you',
      scope: state.scope === 'fleet' ? { type: 'fleet' } : { type: state.scope, value: state.scopeValue },
      window: state.window === 'custom'
        ? { type: 'custom', start: startEl.value, end: endEl.value }
        : state.window
    };
    try {
      var response = await fetch('/api/reports', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(payload)
      });
      var body = await response.json();
      if (!response.ok) throw new Error(body.error || 'report generation failed');
      window.location.href = body.url;
    } catch (err) {
      statusEl.textContent = err.message || String(err);
      generateEl.disabled = false;
    }
  });
  sync();
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reports_persist_across_reopen() {
        let path = std::env::temp_dir().join(format!(
            "glass-report-test-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        {
            let glass = Glass::open(&path).expect("open test db");
            let created = glass
                .create_report(NewReport {
                    kind: "activity-digest".to_string(),
                    scope_type: "fleet".to_string(),
                    scope_value: None,
                    window_start: Some(1),
                    window_end: Some(2),
                    title: "Activity digest - fleet".to_string(),
                    doc_html: "<p>persisted</p>".to_string(),
                    meta_json: json!({"test": true}),
                    requested_by: "test".to_string(),
                })
                .expect("create report");
            assert_eq!(created.id, "R-001");
        }
        {
            let glass = Glass::open(&path).expect("reopen test db");
            let loaded = glass.get_report("R-001").expect("load report");
            assert_eq!(loaded.title, "Activity digest - fleet");
            assert_eq!(loaded.doc_html, "<p>persisted</p>");
            assert_eq!(loaded.meta_json, json!({"test": true}));
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn custom_windows_are_closed_open_local_dates() {
        let window = resolve_custom_window("2026-07-01", "2026-07-08").expect("custom window");
        assert_eq!(window.preset, "custom");
        assert!(window.start < window.end);
        assert_eq!(window.label, "2026-07-01 - 2026-07-08");
    }
}
