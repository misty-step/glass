use anyhow::{Result, anyhow, bail};
use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use glance_catalog::leaf::Metric;
use glance_catalog::structural::{Cell, CellValue, ColumnSpec, Hero, Row, Table};
use glance_catalog::{Component, InlineNode, RenderContext, render_component};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration as StdDuration;
use tracing::{info, warn};

use crate::{
    ApiError, ClipQueueItem, FeedKind, Glass, Post, Session, SurfaceKind, backlog_report,
    declared_summary, feed_kind_for_post, needs_you, rep1, report_components as rc, review_report,
    sanctum_url, shell,
};

const REPORT_CACHE_FRESHNESS_SECONDS: i64 = 30 * 60;

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
    #[serde(default, alias = "regenerate", alias = "force")]
    regenerate: bool,
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
            "fleet" => true,
            // Powder's card-list contract does not identify who completed a
            // card. Treating every completion as the selected agent's work is
            // worse than omitting unattributed evidence from that scope.
            "agent" => false,
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

struct ReportGeneration {
    report: ReportRecord,
    cached: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum StandingDigestCadence {
    Daily,
    Weekly,
}

impl StandingDigestCadence {
    fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }

    fn title_prefix(self) -> &'static str {
        match self {
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
        }
    }
}

#[derive(Debug)]
struct StandingDigestDue {
    run_at: DateTime<Local>,
    cadences: Vec<StandingDigestCadence>,
}

#[derive(Debug)]
enum StandingDigestOutcome {
    Created(Box<ReportRecord>),
    Skipped(Box<ReportRecord>),
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
    components: Option<Vec<rc::ReportComponent>>,
    html: Option<String>,
    status: String,
}

pub(crate) async fn reports_shell(
    State(_glass): State<Glass>,
    Query(query): Query<ReportsPageQuery>,
) -> Result<Html<String>, ApiError> {
    let styles = reports_styles();
    Ok(Html(shell::render_shell(shell::Shell {
        title: "Glass - Reports",
        active: Some(shell::Place::Reports),
        needs_you_count: needs_you::needs_you_count().await,
        sanctum_url: &sanctum_url(),
        styles: &styles,
        body: &render_reports_body(&query),
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
    let styles = reports_styles();
    Ok(Html(shell::render_shell(shell::Shell {
        title: &format!("Glass - {}", report.id),
        active: Some(shell::Place::Reports),
        needs_you_count: needs_you::needs_you_count().await,
        sanctum_url: &sanctum_url(),
        styles: &styles,
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
    let generation = generate_and_persist(&glass, input).await.map_err(|error| {
        crate::api_failure("glass.reports.generate.failed", "/api/reports", error)
    })?;
    let report = generation.report;
    Ok(Json(json!({
        "id": report.id,
        "url": report.url(),
        "title": report.title,
        "html": render_inline_report(&report, generation.cached),
        "cached": generation.cached,
        "generatedAt": report.generated_at,
        "generatedClock": format_generated_clock(report.generated_at),
        "cacheNote": cache_note(report.generated_at, generation.cached),
    })))
}

pub(crate) fn start_standing_digest_scheduler(glass: Glass) {
    tokio::spawn(async move {
        loop {
            let due = next_standing_digest_due_after(Local::now());
            let sleep_for = duration_until(due.run_at);
            tokio::time::sleep(sleep_for).await;
            run_due_standing_digests(&glass, &due).await;
        }
    });
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

async fn generate_and_persist(
    glass: &Glass,
    input: GenerateReportRequest,
) -> Result<ReportGeneration> {
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

    if !input.regenerate {
        let min_generated_at = Some(Utc::now().timestamp() - REPORT_CACHE_FRESHNESS_SECONDS);
        if let Some(report) = glass.find_report_for_query(
            kind.as_str(),
            &scope.scope_type,
            scope.scope_value.as_deref(),
            window.as_ref().map(|window| window.start),
            window.as_ref().map(|window| window.end),
            min_generated_at,
        )? {
            return Ok(ReportGeneration {
                report,
                cached: true,
            });
        }
    }

    let generated = match kind {
        ReportKind::ActivityDigest => {
            generate_activity_digest(glass, &scope, window.as_ref().expect("window")).await?
        }
        ReportKind::FleetDigest => generate_fleet_digest(window.as_ref().expect("window")).await?,
        ReportKind::Backlog => generate_backlog(&scope).await?,
        ReportKind::ReviewIndex => generate_review_index(glass)?,
    };

    let report = glass.create_report(NewReport {
        kind: kind.as_str().to_string(),
        scope_type: scope.scope_type,
        scope_value: scope.scope_value,
        window_start: window.as_ref().map(|window| window.start),
        window_end: window.as_ref().map(|window| window.end),
        title: generated.title,
        doc_html: generated.doc_html,
        meta_json: generated.meta_json,
        requested_by,
    })?;
    Ok(ReportGeneration {
        report,
        cached: false,
    })
}

async fn run_due_standing_digests(glass: &Glass, due: &StandingDigestDue) {
    for cadence in &due.cadences {
        match generate_standing_digest_once(glass, *cadence, due.run_at).await {
            Ok(StandingDigestOutcome::Created(report)) => info!(
                cadence = cadence.as_str(),
                report_id = report.id.as_str(),
                window_start = report.window_start,
                window_end = report.window_end,
                "generated standing Glass digest"
            ),
            Ok(StandingDigestOutcome::Skipped(report)) => info!(
                cadence = cadence.as_str(),
                report_id = report.id.as_str(),
                window_start = report.window_start,
                window_end = report.window_end,
                "skipped standing Glass digest because the window already exists"
            ),
            Err(error) => warn!(
                cadence = cadence.as_str(),
                scheduled_run_at = due.run_at.to_rfc3339(),
                error = %error,
                "skipped standing Glass digest after generation error"
            ),
        }
    }
}

async fn generate_standing_digest_once(
    glass: &Glass,
    cadence: StandingDigestCadence,
    run_at: DateTime<Local>,
) -> Result<StandingDigestOutcome> {
    let window = standing_digest_window(cadence, run_at)?;
    if let Some(report) = existing_activity_digest(glass, &window)? {
        return Ok(StandingDigestOutcome::Skipped(Box::new(report)));
    }

    let scope = ReportScope::fleet();
    let mut generated = generate_activity_digest(glass, &scope, &window).await?;
    generated.title = format!(
        "{} activity digest - {}",
        cadence.title_prefix(),
        scope.label()
    );
    attach_standing_digest_meta(&mut generated.meta_json, cadence, run_at);
    let report = glass.create_report(NewReport {
        kind: ReportKind::ActivityDigest.as_str().to_string(),
        scope_type: scope.scope_type,
        scope_value: scope.scope_value,
        window_start: Some(window.start),
        window_end: Some(window.end),
        title: generated.title,
        doc_html: generated.doc_html,
        meta_json: generated.meta_json,
        requested_by: "glass-standing-digest".to_string(),
    })?;
    Ok(StandingDigestOutcome::Created(Box::new(report)))
}

fn existing_activity_digest(
    glass: &Glass,
    window: &ResolvedWindow,
) -> Result<Option<ReportRecord>> {
    glass.find_activity_digest_report(window.start, window.end)
}

fn attach_standing_digest_meta(
    meta_json: &mut Value,
    cadence: StandingDigestCadence,
    run_at: DateTime<Local>,
) {
    let marker = json!({
        "cadence": cadence.as_str(),
        "scheduledRunAt": run_at.with_timezone(&Utc).to_rfc3339(),
        "localScheduledRunAt": run_at.to_rfc3339(),
    });
    if let Value::Object(object) = meta_json {
        object.insert("standingDigest".to_string(), marker);
    } else {
        let source = std::mem::take(meta_json);
        *meta_json = json!({
            "standingDigest": marker,
            "source": source,
        });
    }
}

fn next_standing_digest_due_after(now: DateTime<Local>) -> StandingDigestDue {
    let daily = next_daily_run_after(now);
    let weekly = next_weekly_run_after(now);
    let run_at = if daily <= weekly { daily } else { weekly };
    let mut cadences = Vec::new();
    if daily == run_at {
        cadences.push(StandingDigestCadence::Daily);
    }
    if weekly == run_at {
        cadences.push(StandingDigestCadence::Weekly);
    }
    StandingDigestDue { run_at, cadences }
}

fn next_daily_run_after(now: DateTime<Local>) -> DateTime<Local> {
    let today = now.date_naive();
    let today_six = local_time(today, 6, 0, 0);
    if now < today_six {
        today_six
    } else {
        local_time(today + Duration::days(1), 6, 0, 0)
    }
}

fn next_weekly_run_after(now: DateTime<Local>) -> DateTime<Local> {
    let today = now.date_naive();
    let this_monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    let this_monday_six = local_time(this_monday, 6, 0, 0);
    if now < this_monday_six {
        this_monday_six
    } else {
        local_time(this_monday + Duration::days(7), 6, 0, 0)
    }
}

fn standing_digest_window(
    cadence: StandingDigestCadence,
    run_at: DateTime<Local>,
) -> Result<ResolvedWindow> {
    let run_date = run_at.date_naive();
    match cadence {
        StandingDigestCadence::Daily => {
            build_window("standing-daily", run_date - Duration::days(1), run_date)
        }
        StandingDigestCadence::Weekly => {
            let week_end =
                run_date - Duration::days(run_date.weekday().num_days_from_monday() as i64);
            build_window("standing-weekly", week_end - Duration::days(7), week_end)
        }
    }
}

fn local_time(date: NaiveDate, hour: u32, minute: u32, second: u32) -> DateTime<Local> {
    let naive = date
        .and_hms_opt(hour, minute, second)
        .expect("scheduler uses valid wall-clock components");
    Local
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&naive).latest())
        .expect("scheduler local wall-clock time must resolve")
}

fn duration_until(run_at: DateTime<Local>) -> StdDuration {
    (run_at.with_timezone(&Utc) - Utc::now())
        .to_std()
        .unwrap_or_else(|_| StdDuration::from_secs(0))
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

fn render_reports_body(query: &ReportsPageQuery) -> String {
    let initial_kind = html_escape(query.kind.as_deref().unwrap_or("activity-digest"));
    let initial_scope = html_escape(query.scope.as_deref().unwrap_or("fleet"));
    format!(
        r#"<div class="reports-shell" data-initial-kind="{initial_kind}" data-initial-scope="{initial_scope}">
  <section class="reports-ask" aria-labelledby="reports-ask-title">
    <p class="ae-plate-cap" id="reports-ask-title">REPORT QUERY</p>
    <p class="reports-sentence">
      <span>Show me</span>
      <select class="reports-slot" id="reports-scope" aria-label="scope">
        <option value="fleet">the whole fleet</option>
        <option value="agent">one agent</option>
        <option value="repo">one repo</option>
      </select>
      <input class="ae-input reports-scope-value" id="reports-scope-value" aria-label="agent or repo" placeholder="agent name">
      <span>over</span>
      <select class="reports-slot" id="reports-window" aria-label="window">
        <option value="past-hour">the past hour</option>
        <option value="past-24h" selected>the past 24h</option>
        <option value="past-week">the past week</option>
        <option value="past-month">the past month</option>
        <option value="custom">custom range</option>
      </select>
      <button class="ae-button ae-button-compact" id="reports-run" type="button">Run</button>
    </p>
    <div class="reports-custom" id="reports-custom">
      <label>Start <input class="ae-input" id="reports-start" type="date"></label>
      <label>End <input class="ae-input" id="reports-end" type="date"></label>
    </div>
    <p class="reports-cache-line"><span id="reports-status" role="status"></span><button class="reports-regenerate" id="reports-regenerate" type="button" hidden>regenerate</button></p>
  </section>
  <section class="reports-result" id="reports-result" aria-live="polite"></section>
</div>"#,
    )
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

fn render_inline_report(report: &ReportRecord, cached: bool) -> String {
    format!(
        r#"<article class="ae-doc reports-doc reports-inline-doc" data-report-id="{id}">
  <header class="reports-doc-head">
    <div class="reports-inline-headline">
      <p class="ae-plate-cap">{scope} - {window}</p>
      <span class="reports-cache-note">{cache_note}</span>
    </div>
    <h1>{title}</h1>
  </header>
  <div class="reports-doc-body">{doc_html}</div>
</article>"#,
        id = html_escape(&report.id),
        scope = html_escape(&scope_label(
            &report.scope_type,
            report.scope_value.as_deref()
        )),
        window = html_escape(&format_report_window(
            report.window_start,
            report.window_end
        )),
        cache_note = html_escape(&cache_note(report.generated_at, cached)),
        title = html_escape(&report.title),
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
    let blocked_posts = posts
        .iter()
        .filter(|item| feed_kind_for_post(&item.post) == FeedKind::Blocked)
        .cloned()
        .collect::<Vec<_>>();
    let blocked_count = posts
        .iter()
        .filter(|item| feed_kind_for_post(&item.post) == FeedKind::Blocked)
        .count();
    let synthesis = fetch_synthesis_components(scope, window, &posts, &clips, &powder).await;
    let components = synthesis.components.unwrap_or_else(|| {
        build_activity_components(scope, window, &posts, &clips, &powder.cards, &blocked_posts)
    });

    let mut html = rc::render_components(&components);
    if let Some(synthesis_html) = synthesis.html.as_deref() {
        html.push_str(
            "<section class=\"reports-synthesis\"><p class=\"ae-plate-cap\">LEGACY SYNTHESIS</p>",
        );
        html.push_str(synthesis_html);
        html.push_str("</section>");
    }

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
            "components": components,
            "powderStatus": powder.status,
            "synthesisStatus": synthesis.status,
        }),
    })
}

fn build_activity_components(
    scope: &ReportScope,
    window: &ResolvedWindow,
    posts: &[ActivityPost],
    clips: &[ClipQueueItem],
    cards: &[PowderCard],
    blocked_posts: &[ActivityPost],
) -> Vec<rc::ReportComponent> {
    let hourly = hourly_activity_series(window, posts, cards);
    let evidence = activity_evidence_links(posts, cards);
    let mut components = vec![
        rc::ReportComponent::Hero {
            kicker: format!("{} · {} · BRIEF", "ACTIVITY DIGEST", window.label),
            headline: activity_headline(scope, posts, cards, blocked_posts),
            figures: vec![
                figure(cards.len(), "completed cards", false),
                figure(posts.len(), "Glass posts", false),
                figure(clips.len(), "clips", false),
                figure(blocked_posts.len(), "blocked", !blocked_posts.is_empty()),
            ],
            trend: hourly.clone(),
            peak_label: Some(activity_peak_label(&hourly)),
        },
        rc::ReportComponent::Prose {
            text: velocity_theme_sentence(scope, window, posts, cards, clips),
        },
        rc::ReportComponent::Pipeline {
            stages: activity_pipeline(posts, cards, blocked_posts),
        },
        rc::ReportComponent::Prose {
            text: evidence_theme_sentence(scope, posts),
        },
        activity_data_exhibit(posts)
            .unwrap_or(rc::ReportComponent::EvidenceChips { links: evidence }),
        rc::ReportComponent::Prose {
            text: risk_theme_sentence(blocked_posts),
        },
        rc::ReportComponent::Callouts {
            lines: blocked_callouts(blocked_posts),
        },
    ];
    components.push(rc::ReportComponent::IconRow {
        rows: activity_decision_rows(scope, window, posts, cards, blocked_posts),
    });
    components.push(rc::ReportComponent::FigCaption {
        text: format!(
            "{} · sources: Glass wire, clips, Powder completions · generated {} · cached {}m",
            scope.label(),
            format_timestamp(Utc::now().timestamp()),
            REPORT_CACHE_FRESHNESS_SECONDS / 60
        ),
    });
    components
}

fn activity_headline(
    scope: &ReportScope,
    posts: &[ActivityPost],
    cards: &[PowderCard],
    blocked_posts: &[ActivityPost],
) -> String {
    let movement = posts.len() + cards.len();
    if blocked_posts.is_empty() {
        format!(
            "{} produced {movement} tracked signal(s) with no blocked Glass post in the window.",
            scope.label()
        )
    } else {
        format!(
            "{} produced {movement} tracked signal(s); {} need attention.",
            scope.label(),
            blocked_posts.len()
        )
    }
}

fn figure(value: usize, label: &str, warn: bool) -> rc::Figure {
    rc::Figure {
        value: value.to_string(),
        label: label.to_string(),
        warn,
    }
}

fn activity_peak_label(series: &[rc::SeriesPoint]) -> String {
    series
        .iter()
        .max_by(|left, right| {
            left.value
                .partial_cmp(&right.value)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|point| format!("peak {} · {}", point.value as usize, point.label))
        .unwrap_or_else(|| "peak n/a".to_string())
}

fn hourly_activity_series(
    window: &ResolvedWindow,
    posts: &[ActivityPost],
    cards: &[PowderCard],
) -> Vec<rc::SeriesPoint> {
    let span = (window.end - window.start).max(1);
    let bucket_count = if span <= 6 * 60 * 60 {
        6
    } else if span <= 36 * 60 * 60 {
        12
    } else {
        14
    };
    let bucket_size = ((span as f64) / (bucket_count as f64)).ceil() as i64;
    let mut buckets = vec![0_usize; bucket_count];
    for ts in posts
        .iter()
        .map(|item| item.post.updated_at.max(item.post.created_at))
        .chain(cards.iter().map(PowderCard::activity_timestamp))
    {
        if ts < window.start || ts >= window.end {
            continue;
        }
        let index = ((ts - window.start) / bucket_size).clamp(0, bucket_count as i64 - 1) as usize;
        buckets[index] += 1;
    }
    buckets
        .into_iter()
        .enumerate()
        .map(|(index, value)| rc::SeriesPoint {
            label: bucket_label(window.start + (index as i64 * bucket_size)),
            value: value as f64,
        })
        .collect()
}

fn velocity_theme_sentence(
    scope: &ReportScope,
    window: &ResolvedWindow,
    posts: &[ActivityPost],
    cards: &[PowderCard],
    clips: &[ClipQueueItem],
) -> String {
    format!(
        "{} over {} resolved into {} Glass post(s), {} completed Powder card(s), and {} clip(s); the pipeline below shows which proof lanes carried the brief.",
        scope.label(),
        window.label,
        posts.len(),
        cards.len(),
        clips.len()
    )
}

fn activity_pipeline(
    posts: &[ActivityPost],
    cards: &[PowderCard],
    blocked_posts: &[ActivityPost],
) -> Vec<rc::PipelineStage> {
    vec![
        rc::PipelineStage {
            label: "wire".to_string(),
            state: if posts.is_empty() {
                rc::PipelineState::Pending
            } else {
                rc::PipelineState::Done
            },
            note: Some(format!("{} post(s)", posts.len())),
        },
        rc::PipelineStage {
            label: "powder".to_string(),
            state: if cards.is_empty() {
                rc::PipelineState::Pending
            } else {
                rc::PipelineState::Done
            },
            note: Some(format!("{} completed", cards.len())),
        },
        rc::PipelineStage {
            label: "risk".to_string(),
            state: if blocked_posts.is_empty() {
                rc::PipelineState::Done
            } else {
                rc::PipelineState::Blocked
            },
            note: Some(if blocked_posts.is_empty() {
                "clear".to_string()
            } else {
                format!("{} blocked", blocked_posts.len())
            }),
        },
        rc::PipelineStage {
            label: "brief".to_string(),
            state: rc::PipelineState::Active,
            note: Some("DOC-13".to_string()),
        },
    ]
}

fn evidence_theme_sentence(scope: &ReportScope, posts: &[ActivityPost]) -> String {
    if let Some(kind) = activity_data_exhibit(posts)
        .as_ref()
        .map(component_kind_label)
    {
        return format!(
            "{} carried a concrete {kind} surface in the wire; the exhibit below is rendered from the posted payload.",
            scope.label()
        );
    }
    format!(
        "{} had no diff or terminal surface in the selected window, so the proof instrument is the bounded evidence chip row from Powder and Glass links.",
        scope.label()
    )
}

fn component_kind_label(component: &rc::ReportComponent) -> &'static str {
    match component {
        rc::ReportComponent::DiffExhibit { .. } => "diff",
        rc::ReportComponent::TerminalExhibit { .. } => "terminal",
        _ => "evidence",
    }
}

fn activity_evidence_links(posts: &[ActivityPost], cards: &[PowderCard]) -> Vec<rc::EvidenceLink> {
    let mut links = cards
        .iter()
        .take(4)
        .map(|card| rc::EvidenceLink {
            label: card.id.clone(),
            href: powder_card_url(&card.id),
        })
        .collect::<Vec<_>>();
    links.extend(posts.iter().take(4).map(|item| rc::EvidenceLink {
        label: item.post.title.clone(),
        href: post_url(&item.post),
    }));
    links
}

fn activity_data_exhibit(posts: &[ActivityPost]) -> Option<rc::ReportComponent> {
    posts
        .iter()
        .find_map(diff_exhibit_from_post)
        .or_else(|| posts.iter().find_map(terminal_exhibit_from_post))
}

fn diff_exhibit_from_post(item: &ActivityPost) -> Option<rc::ReportComponent> {
    item.post.surfaces.iter().find_map(|surface| {
        if surface.kind != SurfaceKind::Diff {
            return None;
        }
        let patch = surface.fields.get("patch").and_then(Value::as_str)?;
        let file = surface
            .fields
            .get("file")
            .and_then(Value::as_str)
            .unwrap_or(&item.post.title)
            .to_string();
        let lines = patch
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(16)
            .map(|line| {
                let state = if line.starts_with('+') && !line.starts_with("+++") {
                    rc::DiffState::Add
                } else if line.starts_with('-') && !line.starts_with("---") {
                    rc::DiffState::Del
                } else {
                    rc::DiffState::Ctx
                };
                rc::DiffLine {
                    state,
                    text: line.to_string(),
                }
            })
            .collect::<Vec<_>>();
        if lines.is_empty() {
            None
        } else {
            Some(rc::ReportComponent::DiffExhibit { file, lines })
        }
    })
}

fn terminal_exhibit_from_post(item: &ActivityPost) -> Option<rc::ReportComponent> {
    item.post.surfaces.iter().find_map(|surface| {
        if surface.kind != SurfaceKind::Terminal {
            return None;
        }
        let text = surface.fields.get("text").and_then(Value::as_str)?;
        let lines = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(14)
            .map(str::to_string)
            .collect::<Vec<_>>();
        if lines.is_empty() {
            None
        } else {
            Some(rc::ReportComponent::TerminalExhibit { lines })
        }
    })
}

fn risk_theme_sentence(blocked_posts: &[ActivityPost]) -> String {
    if blocked_posts.is_empty() {
        "Risk stayed quiet in this window; the callout band is intentionally green rather than omitted.".to_string()
    } else {
        format!(
            "{} blocked or question-shaped post(s) need operator attention; the callout band keeps them above the fold.",
            blocked_posts.len()
        )
    }
}

fn blocked_callouts(posts: &[ActivityPost]) -> Vec<rc::StatusLine> {
    if posts.is_empty() {
        return vec![rc::StatusLine {
            status: Some("ok".to_string()),
            text: "No blocked Glass posts were published in this window.".to_string(),
            href: None,
        }];
    }
    posts
        .iter()
        .take(6)
        .map(|item| rc::StatusLine {
            status: Some("warn".to_string()),
            text: declared_summary(&item.post)
                .unwrap_or_else(|| format!("{} is blocked", item.post.title)),
            href: Some(post_url(&item.post)),
        })
        .collect()
}

fn activity_decision_rows(
    scope: &ReportScope,
    window: &ResolvedWindow,
    posts: &[ActivityPost],
    cards: &[PowderCard],
    blocked_posts: &[ActivityPost],
) -> Vec<rc::IconRowItem> {
    vec![
        rc::IconRowItem {
            icon: Some("ok".to_string()),
            text: format!(
                "{} uses the DOC-13 instrument brief for this generated digest.",
                scope.label()
            ),
            meta: Some(window.label.clone()),
        },
        rc::IconRowItem {
            icon: Some("report".to_string()),
            text: format!(
                "{} wire post(s) and {} Powder completion(s) were included.",
                posts.len(),
                cards.len()
            ),
            meta: Some("scope-filtered".to_string()),
        },
        rc::IconRowItem {
            icon: Some(
                if blocked_posts.is_empty() {
                    "ok"
                } else {
                    "warn"
                }
                .to_string(),
            ),
            text: if blocked_posts.is_empty() {
                "No blocked Glass posts were found in the selected window.".to_string()
            } else {
                format!(
                    "{} blocked Glass post(s) remain visible as callouts.",
                    blocked_posts.len()
                )
            },
            meta: Some("risk band".to_string()),
        },
    ]
}

fn bucket_label(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| ts.to_string())
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

async fn fetch_synthesis_components(
    scope: &ReportScope,
    window: &ResolvedWindow,
    posts: &[ActivityPost],
    clips: &[ClipQueueItem],
    powder: &PowderFetch,
) -> SynthesisFetch {
    let Some(endpoint) = std::env::var("GLASS_SYNTHESIS_ENDPOINT")
        .ok()
        .filter(|value| !value.is_empty())
    else {
        return SynthesisFetch {
            components: None,
            html: None,
            status: "not configured".to_string(),
        };
    };
    let request = json!({
        "window": if window.preset == "custom" { "custom" } else { window.preset.as_str() },
        "since": window.since_rfc3339(),
        "until": window.until_rfc3339(),
        "scope": scope.synthesis_scope(),
        "contract": "glass.report_components.v1",
        "context": synthesis_context_bundle(posts, clips, powder),
    });
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            return SynthesisFetch {
                components: None,
                html: None,
                status: format!("client error: {err}"),
            };
        }
    };
    let response = match client.post(&endpoint).json(&request).send().await {
        Ok(response) => response,
        Err(err) => {
            return SynthesisFetch {
                components: None,
                html: None,
                status: format!("transport error: {err}"),
            };
        }
    };
    if !response.status().is_success() {
        return SynthesisFetch {
            components: None,
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
            Ok(text) => parse_final_sse_payload(&text),
            Err(err) => Err(format!("read synthesis stream: {err}")),
        }
    } else {
        match response.json::<Value>().await {
            Ok(value) => Ok(payload_from_synthesis_value(value)),
            Err(err) => Err(format!("parse synthesis response: {err}")),
        }
    };
    let payload = match value {
        Ok(Some(payload)) => payload,
        Ok(None) => {
            return SynthesisFetch {
                components: None,
                html: None,
                status: "no full synthesis payload in response".to_string(),
            };
        }
        Err(err) => {
            return SynthesisFetch {
                components: None,
                html: None,
                status: err,
            };
        }
    };
    match components_from_payload(&payload) {
        Ok(Some(components)) => SynthesisFetch {
            components: Some(components),
            html: None,
            status: "ok: component-list".to_string(),
        },
        Ok(None) => match rep1::render_spec_value(payload) {
            Ok((html, _)) => SynthesisFetch {
                components: None,
                html: Some(html),
                status: "ok: legacy-retrospec".to_string(),
            },
            Err(err) => SynthesisFetch {
                components: None,
                html: None,
                status: format!("render error: {err}"),
            },
        },
        Err(err) => SynthesisFetch {
            components: None,
            html: None,
            status: err,
        },
    }
}

fn synthesis_context_bundle(
    posts: &[ActivityPost],
    clips: &[ClipQueueItem],
    powder: &PowderFetch,
) -> Value {
    json!({
        "posts": posts.iter().map(|item| {
            json!({
                "id": item.post.id,
                "title": item.post.title,
                "agent": item.session.agent,
                "kind": feed_kind_for_post(&item.post).as_str(),
                "summary": declared_summary(&item.post),
                "updatedAt": item.post.updated_at.max(item.post.created_at),
                "url": post_url(&item.post),
            })
        }).collect::<Vec<_>>(),
        "clips": clips.iter().map(|item| {
            json!({
                "caption": item.draft_caption,
                "agent": item.context.session.agent,
                "createdAt": item.clip.created_at,
                "postUrl": post_url(&item.context.post),
            })
        }).collect::<Vec<_>>(),
        "powder": {
            "status": powder.status,
            "completed": powder.cards.iter().map(|card| {
                json!({
                    "id": card.id,
                    "title": card.title,
                    "repo": card.repo,
                    "priority": card.priority,
                    "completedAt": card.activity_timestamp(),
                    "url": powder_card_url(&card.id),
                })
            }).collect::<Vec<_>>(),
        }
    })
}

fn components_from_payload(value: &Value) -> Result<Option<Vec<rc::ReportComponent>>, String> {
    if let Some(components) = value.get("components") {
        return serde_json::from_value::<Vec<rc::ReportComponent>>(components.clone())
            .map(Some)
            .map_err(|err| format!("parse component-list synthesis payload: {err}"));
    }
    if value.is_array() {
        return serde_json::from_value::<Vec<rc::ReportComponent>>(value.clone())
            .map(Some)
            .map_err(|err| format!("parse component-list synthesis payload: {err}"));
    }
    Ok(None)
}

fn resolve_window(value: &Value) -> Result<ResolvedWindow> {
    match value {
        Value::Null => resolve_preset("past-24h"),
        Value::String(preset) => resolve_preset(preset),
        Value::Object(object) => {
            let preset = object
                .get("preset")
                .or_else(|| object.get("type"))
                .or_else(|| object.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("past-24h");
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
    match preset {
        "past-hour" | "hour" | "1h" => return resolve_relative_preset("past-hour", 60 * 60),
        "past-24h" | "24h" | "day" => return resolve_relative_preset("past-24h", 24 * 60 * 60),
        "past-week" | "week" | "7d" => {
            return resolve_relative_preset("past-week", 7 * 24 * 60 * 60);
        }
        "past-month" | "month" | "30d" => {
            return resolve_relative_preset("past-month", 30 * 24 * 60 * 60);
        }
        _ => {}
    }
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

fn resolve_relative_preset(preset: &str, seconds: i64) -> Result<ResolvedWindow> {
    let now = Utc::now().timestamp();
    let end = round_up_timestamp(now, REPORT_CACHE_FRESHNESS_SECONDS);
    let start = end - seconds;
    Ok(ResolvedWindow {
        preset: preset.to_string(),
        start,
        end,
        label: match preset {
            "past-hour" => "past hour".to_string(),
            "past-24h" => "past 24h".to_string(),
            "past-week" => "past week".to_string(),
            "past-month" => "past month".to_string(),
            _ => preset.to_string(),
        },
    })
}

fn round_up_timestamp(ts: i64, bucket: i64) -> i64 {
    if bucket <= 0 {
        return ts;
    }
    let remainder = ts.rem_euclid(bucket);
    if remainder == 0 {
        ts
    } else {
        ts + bucket - remainder
    }
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

fn parse_final_sse_payload(text: &str) -> Result<Option<Value>, String> {
    let mut event = "message".to_string();
    let mut data = Vec::<String>::new();
    let mut full = None;
    for line in text.replace("\r\n", "\n").replace('\r', "\n").lines() {
        if line.trim().is_empty() {
            if event == "full" && !data.is_empty() {
                let value = serde_json::from_str::<Value>(&data.join("\n"))
                    .map_err(|err| format!("parse full synthesis event: {err}"))?;
                full = payload_from_synthesis_value(value);
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
        full = payload_from_synthesis_value(value);
    }
    Ok(full)
}

fn payload_from_synthesis_value(value: Value) -> Option<Value> {
    if let Some(components) = value.get("components") {
        return Some(json!({ "components": components }));
    }
    if let Some(spec) = value.get("spec") {
        return Some(spec.clone());
    }
    if value.get("stage").is_some() {
        return None;
    }
    Some(value)
}

impl PowderCard {
    fn activity_timestamp(&self) -> i64 {
        self.completed_at.or(self.updated_at).unwrap_or_default()
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

fn format_generated_clock(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| ts.to_string())
}

fn cache_note(ts: i64, cached: bool) -> String {
    if cached {
        format!("cached · generated {}", format_generated_clock(ts))
    } else {
        format!("generated {}", format_generated_clock(ts))
    }
}

fn timestamp_rfc3339(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("unix epoch"))
        .to_rfc3339()
}

pub(crate) fn reports_styles() -> String {
    format!("{}{}", rc::STYLE, REPORTS_STYLE)
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
.reports-shell { max-width: 1040px; margin: 0 auto; padding: var(--ae-space-6) var(--ae-space-5); display: grid; gap: var(--ae-space-6); }
.reports-ask { border-bottom: 1px solid var(--ae-line); padding-bottom: var(--ae-space-5); }
.reports-sentence { display: flex; align-items: baseline; flex-wrap: wrap; gap: 0.5em; margin: 0.4em 0 0; font-weight: var(--ae-w-medium); }
.reports-slot { appearance: none; border: 0; border-bottom: 1px dashed var(--ae-ink-muted); border-radius: 0; background: transparent; color: var(--ae-ink); font: inherit; font-weight: var(--ae-w-medium); padding: 0 1.2em 0 0.1em; cursor: pointer; }
.reports-slot:hover, .reports-slot:focus { border-bottom-color: var(--ae-ink); outline: none; }
.reports-scope-value { display: none; width: min(16rem, 100%); }
.reports-scope-value.is-on { display: inline-block; }
.reports-custom { display: none; grid-template-columns: repeat(2, minmax(12rem, 1fr)); gap: var(--ae-space-3); margin-top: var(--ae-space-4); }
.reports-custom.is-on { display: grid; }
.reports-custom label { display: grid; gap: 0.35rem; color: var(--ae-ink-muted); font-size: 13px; }
.reports-cache-line { min-height: 1.4em; margin: var(--ae-space-3) 0 0; display: flex; align-items: baseline; gap: 0.75em; flex-wrap: wrap; font-family: var(--ae-font-mono); font-size: 11px; letter-spacing: 0.06em; color: var(--ae-ink-faint); }
.reports-regenerate { border: 0; border-bottom: 1px dashed var(--ae-ink-muted); background: transparent; color: var(--ae-ink-muted); font: inherit; letter-spacing: inherit; padding: 0; cursor: pointer; }
.reports-regenerate:hover { color: var(--ae-ink); border-bottom-color: var(--ae-ink); }
.reports-result:empty { min-height: 10rem; border: 1px dashed var(--ae-line); }
.reports-doc { max-width: 980px; margin: 0 auto; padding: var(--ae-space-6) var(--ae-space-5); }
.reports-inline-doc { margin: 0; padding-inline: 0; }
.reports-doc-head { border-bottom: 1px solid var(--ae-line); margin-bottom: var(--ae-space-5); padding-bottom: var(--ae-space-5); }
.reports-doc-head h1 { font-size: clamp(1.5rem, 2.5vw, 2.4rem); line-height: 1.05; margin: 0.2rem 0 var(--ae-space-4); }
.reports-doc-head dl { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: var(--ae-space-3); margin: 0; }
.reports-doc-head dt { color: var(--ae-ink-muted); font-size: 11px; letter-spacing: 0; }
.reports-doc-head dd { margin: 0.2rem 0 0; font-family: var(--ae-font-mono); font-size: 13px; }
.reports-inline-headline { display: flex; align-items: baseline; justify-content: space-between; gap: var(--ae-space-4); flex-wrap: wrap; }
.reports-cache-note { font-family: var(--ae-font-mono); font-size: 11px; letter-spacing: 0.06em; color: var(--ae-ink-faint); }
.reports-doc-body { display: grid; gap: var(--ae-space-5); }
.reports-synthesis { display: grid; gap: var(--ae-space-3); padding: var(--ae-space-4); border: 1px solid var(--ae-line); }
@media (max-width: 720px) {
  .reports-sentence { align-items: stretch; }
  .reports-slot, .reports-scope-value { max-width: 100%; }
  .reports-custom { grid-template-columns: 1fr; }
  .reports-doc-head dl { grid-template-columns: 1fr; }
}
"#;

const REPORTS_SCRIPT: &str = r#"
(function(){
  var root = document.querySelector('.reports-shell');
  if (!root) return;
  var state = {
    window: 'past-24h',
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
  var scopeEl = document.getElementById('reports-scope');
  var windowEl = document.getElementById('reports-window');
  var customEl = document.getElementById('reports-custom');
  var startEl = document.getElementById('reports-start');
  var endEl = document.getElementById('reports-end');
  var scopeValueEl = document.getElementById('reports-scope-value');
  var statusEl = document.getElementById('reports-status');
  var runEl = document.getElementById('reports-run');
  var regenerateEl = document.getElementById('reports-regenerate');
  var resultEl = document.getElementById('reports-result');

  function pad(n) { return String(n).padStart(2, '0'); }
  function isoDate(d) { return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()); }
  function addDays(d, n) { var x = new Date(d.getFullYear(), d.getMonth(), d.getDate()); x.setDate(x.getDate() + n); return x; }
  function defaultRange() {
    var today = new Date();
    today = new Date(today.getFullYear(), today.getMonth(), today.getDate());
    return [addDays(today, -1), today];
  }
  function sync() {
    if (state.kind === 'backlog' && state.scope !== 'repo') {
      state.scope = 'repo';
      if (!state.scopeValue) state.scopeValue = 'glass';
    }
    scopeEl.value = state.scope;
    windowEl.value = state.window;
    customEl.classList.toggle('is-on', state.window === 'custom');
    scopeValueEl.classList.toggle('is-on', state.scope !== 'fleet');
    scopeValueEl.placeholder = state.scope === 'agent' ? 'agent name' : 'repo name';
    if (state.scope !== 'fleet') scopeValueEl.value = state.scopeValue;
    var range = defaultRange();
    if (!startEl.value) startEl.value = isoDate(range[0]);
    if (!endEl.value) endEl.value = isoDate(range[1]);
  }
  scopeEl.addEventListener('change', function(){ state.scope = scopeEl.value; sync(); });
  windowEl.addEventListener('change', function(){ state.window = windowEl.value; sync(); });
  scopeValueEl.addEventListener('input', function(){ state.scopeValue = scopeValueEl.value.trim(); });
  async function run(force) {
    statusEl.textContent = force ? 'regenerating...' : 'running...';
    runEl.disabled = true;
    regenerateEl.hidden = true;
    state.scopeValue = scopeValueEl.value.trim();
    var payload = {
      kind: state.kind,
      requestedBy: 'you',
      regenerate: !!force,
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
      resultEl.innerHTML = body.html || '';
      statusEl.textContent = body.cacheNote || '';
      regenerateEl.hidden = !body.id;
    } catch (err) {
      statusEl.textContent = err.message || String(err);
    } finally {
      runEl.disabled = false;
    }
  }
  runEl.addEventListener('click', function(){ run(false); });
  regenerateEl.addEventListener('click', function(){ run(true); });
  startEl.addEventListener('input', sync);
  endEl.addEventListener('input', sync);
  if (initialScope.indexOf(':') === -1 && initialScope !== 'fleet') {
    state.scope = initialScope;
  }
  if (state.scope !== 'fleet' && !state.scopeValue) {
    state.scopeValue = state.scope === 'repo' ? 'glass' : '';
  }
  try {
    var url = new URL(window.location.href);
    var win = url.searchParams.get('window');
    if (win) state.window = win;
  } catch (e) {
  }
  sync();
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn local_dt(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .expect("test datetime must exist in local timezone")
    }

    #[test]
    fn reports_persist_across_reopen() {
        let path = temp_db_path("glass-report-test");
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
    fn standing_digest_next_run_math_uses_local_six_am_boundaries() {
        let before_daily = local_dt(2026, 7, 8, 5, 30);
        let next_daily = next_daily_run_after(before_daily);
        assert_eq!(next_daily.date_naive(), before_daily.date_naive());
        assert_eq!(next_daily.hour(), 6);
        assert_eq!(next_daily.minute(), 0);

        let after_daily = local_dt(2026, 7, 8, 6, 1);
        let next_daily = next_daily_run_after(after_daily);
        assert_eq!(
            next_daily.date_naive(),
            after_daily.date_naive() + Duration::days(1)
        );
        assert_eq!(next_daily.hour(), 6);

        let monday_before = local_dt(2026, 7, 13, 5, 59);
        let next_weekly = next_weekly_run_after(monday_before);
        assert_eq!(next_weekly.date_naive(), monday_before.date_naive());
        assert_eq!(next_weekly.weekday().num_days_from_monday(), 0);
        assert_eq!(next_weekly.hour(), 6);

        let monday_after = local_dt(2026, 7, 13, 6, 1);
        let next_weekly = next_weekly_run_after(monday_after);
        assert_eq!(
            next_weekly.date_naive(),
            monday_after.date_naive() + Duration::days(7)
        );
        assert_eq!(next_weekly.weekday().num_days_from_monday(), 0);

        let due = next_standing_digest_due_after(local_dt(2026, 7, 13, 5, 59));
        assert_eq!(
            due.cadences,
            vec![StandingDigestCadence::Daily, StandingDigestCadence::Weekly]
        );
    }

    #[test]
    fn standing_digest_windows_cover_previous_day_and_previous_week() {
        let run_at = local_dt(2026, 7, 8, 6, 0);
        let daily =
            standing_digest_window(StandingDigestCadence::Daily, run_at).expect("daily window");
        assert_eq!(daily.preset, "standing-daily");
        assert_eq!(daily.label, "2026-07-07 - 2026-07-08");

        let run_at = local_dt(2026, 7, 13, 6, 0);
        let weekly =
            standing_digest_window(StandingDigestCadence::Weekly, run_at).expect("weekly window");
        assert_eq!(weekly.preset, "standing-weekly");
        assert_eq!(weekly.label, "2026-07-06 - 2026-07-13");
    }

    #[tokio::test]
    async fn standing_digest_generation_skips_an_existing_window_in_the_real_store() {
        let path = temp_db_path("glass-standing-digest-test");
        let glass = Glass::open(&path).expect("open test db");
        let run_at = local_dt(2026, 7, 8, 6, 0);

        let first = generate_standing_digest_once(&glass, StandingDigestCadence::Daily, run_at)
            .await
            .expect("first digest");
        let StandingDigestOutcome::Created(first_report) = first else {
            panic!("first digest should create a report");
        };
        assert_eq!(first_report.id, "R-001");
        assert_eq!(first_report.title, "Daily activity digest - fleet");
        assert_eq!(first_report.meta_json["standingDigest"]["cadence"], "daily");

        let second = generate_standing_digest_once(&glass, StandingDigestCadence::Daily, run_at)
            .await
            .expect("second digest");
        let StandingDigestOutcome::Skipped(existing_report) = second else {
            panic!("second digest should skip the existing window");
        };
        assert_eq!(existing_report.id, "R-001");

        let reports = glass.list_reports().expect("list reports");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].requested_by, "glass-standing-digest");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn custom_windows_are_closed_open_local_dates() {
        let window = resolve_custom_window("2026-07-01", "2026-07-08").expect("custom window");
        assert_eq!(window.preset, "custom");
        assert!(window.start < window.end);
        assert_eq!(window.label, "2026-07-01 - 2026-07-08");
    }

    #[test]
    fn agent_scope_does_not_claim_unattributed_powder_completions() {
        let card = PowderCard {
            id: "powder-001".to_string(),
            title: "Shipped elsewhere".to_string(),
            status: "done".to_string(),
            repo: Some("powder".to_string()),
            priority: Some("p1".to_string()),
            updated_at: Some(1),
            completed_at: Some(1),
        };

        assert!(ReportScope::fleet().matches_card(&card));
        assert!(
            !ReportScope::new("agent", Some("lead-daybook".to_string()))
                .expect("agent scope")
                .matches_card(&card),
            "Powder's list response has no completion author, so an agent report must not attribute the card"
        );
    }
}
