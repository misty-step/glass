use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::{self, Display};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::http::header::{self, HeaderMap, HeaderValue};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use glance_catalog::leaf::Metric;
use glance_catalog::structural::{Cell, CellValue, ColumnSpec, Hero, Row, Table};
use glance_catalog::{
    Component, InlineNode, REPORT, RenderContext, render_component, validate_layout,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tower_http::catch_panic::CatchPanicLayer;

mod backlog_report;
pub mod canary;
mod needs_you;
mod rep1;
mod reports;
mod review_report;
mod shell;
mod window_report;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub const SURFACE_KINDS: [SurfaceKind; 10] = [
    SurfaceKind::Html,
    SurfaceKind::Diff,
    SurfaceKind::Image,
    SurfaceKind::Trace,
    SurfaceKind::Markdown,
    SurfaceKind::Terminal,
    SurfaceKind::Mermaid,
    SurfaceKind::Json,
    SurfaceKind::Code,
    SurfaceKind::Metric,
];

pub const FEED_KINDS: [FeedKind; 8] = [
    FeedKind::Shipped,
    FeedKind::Report,
    FeedKind::Blocked,
    FeedKind::Question,
    FeedKind::Note,
    FeedKind::Digest,
    FeedKind::Release,
    FeedKind::Receipt,
];

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceKind {
    Html,
    Diff,
    Image,
    Trace,
    Markdown,
    Terminal,
    Mermaid,
    Json,
    Code,
    Metric,
}

impl SurfaceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SurfaceKind::Html => "html",
            SurfaceKind::Diff => "diff",
            SurfaceKind::Image => "image",
            SurfaceKind::Trace => "trace",
            SurfaceKind::Markdown => "markdown",
            SurfaceKind::Terminal => "terminal",
            SurfaceKind::Mermaid => "mermaid",
            SurfaceKind::Json => "json",
            SurfaceKind::Code => "code",
            SurfaceKind::Metric => "metric",
        }
    }

    pub fn required_field(self) -> Option<&'static str> {
        match self {
            SurfaceKind::Html => Some("html"),
            SurfaceKind::Markdown => Some("markdown"),
            SurfaceKind::Mermaid => Some("mermaid"),
            SurfaceKind::Terminal => Some("text"),
            SurfaceKind::Code => Some("code"),
            SurfaceKind::Image => Some("asset_id"),
            SurfaceKind::Json => Some("data"),
            SurfaceKind::Diff | SurfaceKind::Trace | SurfaceKind::Metric => None,
        }
    }

    pub fn sandboxed(self) -> bool {
        matches!(
            self,
            SurfaceKind::Html
                | SurfaceKind::Diff
                | SurfaceKind::Markdown
                | SurfaceKind::Terminal
                | SurfaceKind::Mermaid
                | SurfaceKind::Code
        )
    }
}

impl Display for SurfaceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SurfaceKind {
    type Err = anyhow::Error;

    fn from_str(raw: &str) -> Result<Self> {
        SURFACE_KINDS
            .iter()
            .copied()
            .find(|kind| kind.as_str() == raw)
            .ok_or_else(|| anyhow!("unknown surface kind: {raw}"))
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedKind {
    Shipped,
    #[default]
    Report,
    Blocked,
    Question,
    Note,
    Digest,
    Release,
    Receipt,
}

impl FeedKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FeedKind::Shipped => "shipped",
            FeedKind::Report => "report",
            FeedKind::Blocked => "blocked",
            FeedKind::Question => "question",
            FeedKind::Note => "note",
            FeedKind::Digest => "digest",
            FeedKind::Release => "release",
            FeedKind::Receipt => "receipt",
        }
    }
}

impl Display for FeedKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FeedKind {
    type Err = anyhow::Error;

    fn from_str(raw: &str) -> Result<Self> {
        FEED_KINDS
            .iter()
            .copied()
            .find(|kind| kind.as_str() == raw)
            .ok_or_else(|| anyhow!("unknown feed kind: {raw}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    #[serde(default)]
    pub id: String,
    pub kind: SurfaceKind,
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}

impl Surface {
    pub fn new(kind: SurfaceKind, payload: Value) -> Result<Self> {
        let Value::Object(mut fields) = payload else {
            bail!("surface payload must be a JSON object");
        };
        fields.remove("kind");
        let id = fields
            .remove("id")
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        let surface = Self { id, kind, fields };
        surface.validate()?;
        Ok(surface)
    }

    fn normalize(mut self, index: usize) -> Result<Self> {
        if self.id.trim().is_empty() {
            self.id = format!("surface-{}", index + 1);
        }
        self.validate()?;
        Ok(self)
    }

    fn validate(&self) -> Result<()> {
        if let Some(field) = self.kind.required_field()
            && !self.fields.contains_key(field)
        {
            bail!("{} surface requires `{field}`", self.kind);
        }
        if self.kind == SurfaceKind::Diff
            && !self.fields.contains_key("patch")
            && !self.fields.contains_key("files")
        {
            bail!("diff surface requires `patch` or `files`");
        }
        if self.kind == SurfaceKind::Metric
            && (!self.fields.contains_key("label") || !self.fields.contains_key("value"))
        {
            bail!("metric surface requires `label` and `value`");
        }
        Ok(())
    }

    fn text_field(&self, field: &str) -> &str {
        self.fields
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub agent: String,
    pub title: String,
    pub cwd: Option<String>,
    pub created_at: i64,
    pub last_active_at: i64,
}

/// A session is demoted out of the primary rail once it has gone this long
/// without a new post. Dead sessions still render on their own agent feed;
/// they just stop appearing as peers of live work at the top of the stage.
const LIVE_WINDOW_SECONDS: i64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub surfaces: Vec<Surface>,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: i64,
    pub history: Vec<PostVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostVersion {
    pub version: i64,
    pub title: String,
    pub surfaces: Vec<Surface>,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub content_type: String,
    pub byte_length: usize,
    pub filename: Option<String>,
    pub created_at: i64,
    pub last_accessed_at: i64,
}

#[derive(Debug, Clone)]
pub struct DoctorConfig {
    pub url: String,
    pub db_path: PathBuf,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    pub url: String,
    pub db_path: PathBuf,
    pub session_count: usize,
    pub probe_session_id: String,
    pub probe_post_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSummary {
    pub agent: String,
    pub post_count: usize,
    pub session_count: usize,
    pub last_active_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSession {
    pub agent: String,
    pub title: String,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishPost {
    #[serde(default, alias = "sessionId")]
    pub session_id: Option<String>,
    #[serde(default, alias = "sessionTitle")]
    pub session_title: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    pub title: String,
    pub surfaces: Vec<Surface>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishOutcome {
    pub post: Post,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipRange {
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
}

impl ClipRange {
    fn validate(&self) -> Result<()> {
        if self.start.is_none() && self.end.is_none() {
            bail!("clip range requires start or end");
        }
        if self.start.is_some_and(|value| value < 0) || self.end.is_some_and(|value| value < 0) {
            bail!("clip range values must be non-negative");
        }
        if let (Some(start), Some(end)) = (self.start, self.end)
            && start > end
        {
            bail!("clip range start must be <= end");
        }
        Ok(())
    }

    fn label(&self) -> Option<String> {
        match (self.start, self.end) {
            (Some(start), Some(end)) => Some(format!("{start}s-{end}s")),
            (Some(start), None) => Some(format!("from {start}s")),
            (None, Some(end)) => Some(format!("until {end}s")),
            (None, None) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub session_id: String,
    pub post_id: String,
    pub post_version: i64,
    pub surface_id: Option<String>,
    pub surface_index: Option<usize>,
    pub range: Option<ClipRange>,
    pub note: Option<String>,
    pub caption: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureClip {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    #[serde(alias = "postId")]
    pub post_id: String,
    #[serde(default, alias = "surfaceId")]
    pub surface_id: Option<String>,
    #[serde(default, alias = "surfaceIndex")]
    pub surface_index: Option<usize>,
    #[serde(default)]
    pub range: Option<ClipRange>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipEvidenceLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipContext {
    pub session: Session,
    pub post: Post,
    pub post_version: i64,
    pub surface: Option<Surface>,
    pub surface_index: Option<usize>,
    pub evidence_links: Vec<ClipEvidenceLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipQueueItem {
    pub clip: Clip,
    pub context: ClipContext,
    pub draft_caption: String,
}

#[derive(Clone)]
pub struct Glass {
    inner: Arc<Mutex<Store>>,
}

struct Store {
    conn: Connection,
}

impl Glass {
    pub fn memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory().context("open in-memory sqlite")?)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create db directory {}", parent.display()))?;
        }
        Self::from_connection(
            Connection::open(path)
                .with_context(|| format!("open sqlite database {}", path.display()))?,
        )
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        let store = Store { conn };
        store.migrate()?;
        Ok(Self {
            inner: Arc::new(Mutex::new(store)),
        })
    }

    pub fn create_session(&self, input: NewSession) -> Result<Session> {
        let mut store = self.lock()?;
        store.create_session(input)
    }

    pub fn publish_post(&self, input: PublishPost) -> Result<PublishOutcome> {
        let mut store = self.lock()?;
        store.publish_post(input)
    }

    pub fn update_post(&self, id: &str, input: PublishPost) -> Result<PublishOutcome> {
        let mut store = self.lock()?;
        store.update_post(id, input)
    }

    pub fn get_post(&self, id: &str) -> Result<Post> {
        let store = self.lock()?;
        store.get_post(id)
    }

    pub fn list_recent_posts(&self, limit: usize) -> Result<Vec<Post>> {
        let store = self.lock()?;
        store.list_recent_posts(limit)
    }

    pub fn capture_clip(&self, input: CaptureClip) -> Result<ClipQueueItem> {
        let mut store = self.lock()?;
        store.capture_clip(input)
    }

    pub fn list_clip_queue(&self, limit: usize) -> Result<Vec<ClipQueueItem>> {
        let store = self.lock()?;
        store.list_clip_queue(limit)
    }

    pub(crate) fn create_report(&self, input: reports::NewReport) -> Result<reports::ReportRecord> {
        let mut store = self.lock()?;
        store.create_report(input)
    }

    pub(crate) fn get_report(&self, id: &str) -> Result<reports::ReportRecord> {
        let store = self.lock()?;
        store.get_report(id)
    }

    pub(crate) fn list_reports(&self) -> Result<Vec<reports::ReportRecord>> {
        let store = self.lock()?;
        store.list_reports()
    }

    pub(crate) fn find_activity_digest_report(
        &self,
        window_start: i64,
        window_end: i64,
    ) -> Result<Option<reports::ReportRecord>> {
        let store = self.lock()?;
        store.find_activity_digest_report(window_start, window_end)
    }

    pub(crate) fn list_activity_posts(
        &self,
        start: i64,
        end: i64,
    ) -> Result<Vec<reports::ActivityPost>> {
        let store = self.lock()?;
        store.list_activity_posts(start, end)
    }

    pub(crate) fn list_activity_clips(&self, start: i64, end: i64) -> Result<Vec<ClipQueueItem>> {
        let store = self.lock()?;
        store.list_activity_clips(start, end)
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let store = self.lock()?;
        store.list_sessions()
    }

    /// Removes a diagnostic probe session and everything under it (posts).
    /// Used only by the doctor to self-clean after it has proven the round
    /// trip; not exposed over HTTP.
    pub fn delete_probe_session(&self, session_id: &str) -> Result<()> {
        let mut store = self.lock()?;
        store.delete_probe_session(session_id)
    }

    pub fn store_asset(
        &self,
        content_type: &str,
        filename: Option<&str>,
        bytes: &[u8],
    ) -> Result<Asset> {
        let mut store = self.lock()?;
        store.store_asset(content_type, filename, bytes)
    }

    fn load_asset(&self, id: &str) -> Result<(Asset, Vec<u8>)> {
        let mut store = self.lock()?;
        store.load_asset(id)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Store>> {
        self.inner
            .lock()
            .map_err(|_| anyhow!("glass store lock poisoned"))
    }
}

impl Store {
    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS sessions (
              id TEXT PRIMARY KEY,
              agent TEXT NOT NULL,
              title TEXT NOT NULL,
              cwd TEXT,
              created_at INTEGER NOT NULL,
              last_active_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS posts (
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL REFERENCES sessions(id),
              title TEXT NOT NULL,
              surfaces_json TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              version INTEGER NOT NULL,
              history_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS assets (
              id TEXT PRIMARY KEY,
              content_type TEXT NOT NULL,
              byte_length INTEGER NOT NULL,
              filename TEXT,
              data BLOB NOT NULL,
              created_at INTEGER NOT NULL,
              last_accessed_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS clips (
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL REFERENCES sessions(id),
              post_id TEXT NOT NULL REFERENCES posts(id),
              post_version INTEGER NOT NULL,
              surface_id TEXT,
              surface_index INTEGER,
              range_json TEXT,
              note TEXT,
              caption TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS clips_created_at_idx
              ON clips(created_at DESC);
            CREATE INDEX IF NOT EXISTS clips_session_idx
              ON clips(session_id, post_id);
            CREATE TABLE IF NOT EXISTS reports (
              id TEXT PRIMARY KEY,
              kind TEXT NOT NULL,
              scope_type TEXT NOT NULL,
              scope_value TEXT,
              window_start INTEGER,
              window_end INTEGER,
              title TEXT NOT NULL,
              doc_html TEXT NOT NULL,
              meta_json TEXT NOT NULL,
              generated_at INTEGER NOT NULL,
              requested_by TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS reports_generated_at_idx
              ON reports(generated_at DESC, id DESC);
            "#,
        )?;
        // Glass is ONE-WAY as of glass-912: the comment/feedback surface and
        // its agent_seq cursor are deleted, not hidden. Drop them from any
        // database created under the earlier two-way schema.
        self.conn.execute_batch("DROP TABLE IF EXISTS comments;")?;
        let has_agent_seq = self
            .conn
            .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'agent_seq'")?
            .exists([])?;
        if has_agent_seq {
            self.conn
                .execute_batch("ALTER TABLE sessions DROP COLUMN agent_seq;")?;
        }
        Ok(())
    }

    fn create_session(&mut self, input: NewSession) -> Result<Session> {
        if input.agent.trim().is_empty() {
            bail!("agent is required");
        }
        if input.title.trim().is_empty() {
            bail!("session title is required");
        }
        let now = now_seconds();
        let session = Session {
            id: fresh_id("ses"),
            agent: input.agent,
            title: input.title,
            cwd: input.cwd,
            created_at: now,
            last_active_at: now,
        };
        self.conn.execute(
            "INSERT INTO sessions (id, agent, title, cwd, created_at, last_active_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session.id,
                session.agent,
                session.title,
                session.cwd,
                session.created_at,
                session.last_active_at,
            ],
        )?;
        Ok(session)
    }

    fn ensure_session(&mut self, input: &PublishPost) -> Result<Session> {
        if let Some(id) = input.session_id.as_deref() {
            return self.get_session(id);
        }
        self.create_session(NewSession {
            agent: input.agent.clone().unwrap_or_else(|| "agent".into()),
            title: input
                .session_title
                .clone()
                .unwrap_or_else(|| input.title.clone()),
            cwd: std::env::current_dir()
                .ok()
                .map(|path| path.display().to_string()),
        })
    }

    fn publish_post(&mut self, input: PublishPost) -> Result<PublishOutcome> {
        let session = self.ensure_session(&input)?;
        let surfaces = normalize_surfaces(input.surfaces)?;
        let now = now_seconds();
        let post = Post {
            id: fresh_id("post"),
            session_id: session.id.clone(),
            title: input.title,
            surfaces,
            created_at: now,
            updated_at: now,
            version: 1,
            history: Vec::new(),
        };
        self.conn.execute(
            "INSERT INTO posts (id, session_id, title, surfaces_json, created_at, updated_at, version, history_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                post.id,
                post.session_id,
                post.title,
                serde_json::to_string(&post.surfaces)?,
                post.created_at,
                post.updated_at,
                post.version,
                serde_json::to_string(&post.history)?,
            ],
        )?;
        self.touch_session(&post.session_id)?;
        Ok(PublishOutcome {
            url: format!("/session/{}/p/{}", post.session_id, post.id),
            post,
        })
    }

    fn update_post(&mut self, id: &str, input: PublishPost) -> Result<PublishOutcome> {
        let existing = self.get_post(id)?;
        let mut history = existing.history.clone();
        history.push(PostVersion {
            version: existing.version,
            title: existing.title,
            surfaces: existing.surfaces,
            at: existing.updated_at,
        });
        if history.len() > 20 {
            history.remove(0);
        }
        let now = now_seconds();
        let post = Post {
            id: existing.id,
            session_id: existing.session_id,
            title: input.title,
            surfaces: normalize_surfaces(input.surfaces)?,
            created_at: existing.created_at,
            updated_at: now,
            version: existing.version + 1,
            history,
        };
        self.conn.execute(
            "UPDATE posts
             SET title = ?2, surfaces_json = ?3, updated_at = ?4, version = ?5, history_json = ?6
             WHERE id = ?1",
            params![
                post.id,
                post.title,
                serde_json::to_string(&post.surfaces)?,
                post.updated_at,
                post.version,
                serde_json::to_string(&post.history)?,
            ],
        )?;
        self.touch_session(&post.session_id)?;
        Ok(PublishOutcome {
            url: format!("/session/{}/p/{}", post.session_id, post.id),
            post,
        })
    }

    fn get_session(&self, id: &str) -> Result<Session> {
        self.conn
            .query_row(
                "SELECT id, agent, title, cwd, created_at, last_active_at FROM sessions WHERE id = ?1",
                [id],
                row_to_session,
            )
            .optional()?
            .ok_or_else(|| anyhow!("session not found: {id}"))
    }

    fn list_sessions(&self) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, title, cwd, created_at, last_active_at
             FROM sessions ORDER BY last_active_at DESC, created_at DESC LIMIT 100",
        )?;
        let sessions = stmt
            .query_map([], row_to_session)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(sessions)
    }

    fn get_post(&self, id: &str) -> Result<Post> {
        self.conn
            .query_row(
                "SELECT id, session_id, title, surfaces_json, created_at, updated_at, version, history_json
                 FROM posts WHERE id = ?1",
                [id],
                row_to_post,
            )
            .optional()?
            .ok_or_else(|| anyhow!("post not found: {id}"))
    }

    fn list_recent_posts(&self, limit: usize) -> Result<Vec<Post>> {
        let limit = limit.clamp(1, 100) as i64;
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, title, surfaces_json, created_at, updated_at, version, history_json
             FROM posts ORDER BY updated_at DESC, created_at DESC LIMIT ?1",
        )?;
        let posts = stmt
            .query_map([limit], row_to_post)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(posts)
    }

    fn capture_clip(&mut self, input: CaptureClip) -> Result<ClipQueueItem> {
        if input.session_id.trim().is_empty() {
            bail!("session_id is required");
        }
        if input.post_id.trim().is_empty() {
            bail!("post_id is required");
        }
        if let Some(range) = &input.range {
            range.validate()?;
        }
        let session = self.get_session(&input.session_id)?;
        let post = self.get_post(&input.post_id)?;
        if post.session_id != session.id {
            bail!("post {} does not belong to session {}", post.id, session.id);
        }
        let surface_id = normalize_optional_text(input.surface_id);
        let surface_index = input.surface_index;
        let (_, surface) =
            resolve_surface_ref(&post.surfaces, surface_id.as_deref(), surface_index)?;
        let note = normalize_optional_text(input.note);
        let caption = draft_clip_caption(
            &session,
            &post,
            surface.as_ref(),
            input.range.as_ref(),
            note.as_deref(),
        );
        let now = now_seconds();
        let clip = Clip {
            id: fresh_id("clip"),
            session_id: session.id.clone(),
            post_id: post.id.clone(),
            post_version: post.version,
            surface_id,
            surface_index,
            range: input.range,
            note,
            caption,
            created_at: now,
        };
        self.conn.execute(
            "INSERT INTO clips (id, session_id, post_id, post_version, surface_id, surface_index, range_json, note, caption, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                clip.id,
                clip.session_id,
                clip.post_id,
                clip.post_version,
                clip.surface_id,
                clip.surface_index.map(|index| index as i64),
                clip.range
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                clip.note,
                clip.caption,
                clip.created_at,
            ],
        )?;
        self.clip_queue_item(clip)
    }

    fn list_clip_queue(&self, limit: usize) -> Result<Vec<ClipQueueItem>> {
        let limit = limit.clamp(1, 100) as i64;
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, post_id, post_version, surface_id, surface_index, range_json, note, caption, created_at
             FROM clips ORDER BY created_at DESC, id DESC LIMIT ?1",
        )?;
        let clips = stmt
            .query_map([limit], row_to_clip)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        clips
            .into_iter()
            .map(|clip| self.clip_queue_item(clip))
            .collect()
    }

    fn create_report(&mut self, input: reports::NewReport) -> Result<reports::ReportRecord> {
        if input.kind.trim().is_empty() {
            bail!("report kind is required");
        }
        if input.scope_type.trim().is_empty() {
            bail!("report scope_type is required");
        }
        if input.title.trim().is_empty() {
            bail!("report title is required");
        }
        if input.doc_html.trim().is_empty() {
            bail!("report doc_html is required");
        }
        let generated_at = now_seconds();
        let mut next = self.next_report_counter()?;
        loop {
            let id = format!("R-{next:03}");
            let inserted = self.conn.execute(
                "INSERT OR IGNORE INTO reports
                 (id, kind, scope_type, scope_value, window_start, window_end, title, doc_html, meta_json, generated_at, requested_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    id,
                    &input.kind,
                    &input.scope_type,
                    &input.scope_value,
                    &input.window_start,
                    &input.window_end,
                    &input.title,
                    &input.doc_html,
                    serde_json::to_string(&input.meta_json)?,
                    generated_at,
                    &input.requested_by,
                ],
            )?;
            if inserted == 1 {
                return self.get_report(&id);
            }
            next += 1;
        }
    }

    fn next_report_counter(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COALESCE(MAX(CAST(SUBSTR(id, 3) AS INTEGER)), 0) + 1
                 FROM reports WHERE id GLOB 'R-[0-9]*'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    fn get_report(&self, id: &str) -> Result<reports::ReportRecord> {
        self.conn
            .query_row(
                "SELECT id, kind, scope_type, scope_value, window_start, window_end, title, doc_html, meta_json, generated_at, requested_by
                 FROM reports WHERE id = ?1",
                [id],
                row_to_report,
            )
            .optional()?
            .ok_or_else(|| anyhow!("report not found: {id}"))
    }

    fn list_reports(&self) -> Result<Vec<reports::ReportRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, scope_type, scope_value, window_start, window_end, title, doc_html, meta_json, generated_at, requested_by
             FROM reports ORDER BY generated_at DESC, id DESC LIMIT 500",
        )?;
        let reports = stmt
            .query_map([], row_to_report)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(reports)
    }

    fn find_activity_digest_report(
        &self,
        window_start: i64,
        window_end: i64,
    ) -> Result<Option<reports::ReportRecord>> {
        self.conn
            .query_row(
                "SELECT id, kind, scope_type, scope_value, window_start, window_end, title, doc_html, meta_json, generated_at, requested_by
                 FROM reports
                 WHERE kind = 'activity-digest'
                   AND scope_type = 'fleet'
                   AND scope_value IS NULL
                   AND window_start = ?1
                   AND window_end = ?2
                 ORDER BY generated_at DESC, id DESC
                 LIMIT 1",
                params![window_start, window_end],
                row_to_report,
            )
            .optional()
            .map_err(Into::into)
    }

    fn list_activity_posts(&self, start: i64, end: i64) -> Result<Vec<reports::ActivityPost>> {
        let mut stmt = self.conn.prepare(
            "SELECT
               p.id, p.session_id, p.title, p.surfaces_json, p.created_at, p.updated_at, p.version, p.history_json,
               s.id, s.agent, s.title, s.cwd, s.created_at, s.last_active_at
             FROM posts p
             JOIN sessions s ON s.id = p.session_id
             WHERE MAX(p.created_at, p.updated_at) >= ?1
               AND MAX(p.created_at, p.updated_at) < ?2
             ORDER BY MAX(p.created_at, p.updated_at) DESC, p.id DESC",
        )?;
        let posts = stmt
            .query_map(params![start, end], |row| {
                Ok(reports::ActivityPost {
                    post: row_to_post(row)?,
                    session: Session {
                        id: row.get(8)?,
                        agent: row.get(9)?,
                        title: row.get(10)?,
                        cwd: row.get(11)?,
                        created_at: row.get(12)?,
                        last_active_at: row.get(13)?,
                    },
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(posts)
    }

    fn list_activity_clips(&self, start: i64, end: i64) -> Result<Vec<ClipQueueItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, post_id, post_version, surface_id, surface_index, range_json, note, caption, created_at
             FROM clips
             WHERE created_at >= ?1 AND created_at < ?2
             ORDER BY created_at DESC, id DESC",
        )?;
        let clips = stmt
            .query_map(params![start, end], row_to_clip)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        clips
            .into_iter()
            .map(|clip| self.clip_queue_item(clip))
            .collect()
    }

    fn clip_queue_item(&self, clip: Clip) -> Result<ClipQueueItem> {
        let session = self.get_session(&clip.session_id)?;
        let post = self.get_post(&clip.post_id)?;
        let version_surfaces = post_surfaces_for_version(&post, clip.post_version);
        let (surface_index, surface) = version_surfaces
            .and_then(|surfaces| {
                find_surface_ref(surfaces, clip.surface_id.as_deref(), clip.surface_index)
            })
            .unwrap_or((clip.surface_index, None));
        let evidence_links = clip_evidence_links(&clip, surface_index);
        let post_version = clip.post_version;
        Ok(ClipQueueItem {
            draft_caption: clip.caption.clone(),
            clip,
            context: ClipContext {
                session,
                post,
                post_version,
                surface,
                surface_index,
                evidence_links,
            },
        })
    }

    fn store_asset(
        &mut self,
        content_type: &str,
        filename: Option<&str>,
        bytes: &[u8],
    ) -> Result<Asset> {
        let id = hex::encode(Sha256::digest(bytes));
        let now = now_seconds();
        let existing = self
            .conn
            .query_row(
                "SELECT id, content_type, byte_length, filename, created_at, last_accessed_at FROM assets WHERE id = ?1",
                [&id],
                row_to_asset,
            )
            .optional()?;
        if let Some(mut asset) = existing {
            asset.last_accessed_at = now;
            self.conn.execute(
                "UPDATE assets SET last_accessed_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
            return Ok(asset);
        }
        let asset = Asset {
            id,
            content_type: content_type.to_string(),
            byte_length: bytes.len(),
            filename: filename.map(str::to_owned),
            created_at: now,
            last_accessed_at: now,
        };
        self.conn.execute(
            "INSERT INTO assets (id, content_type, byte_length, filename, data, created_at, last_accessed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                asset.id,
                asset.content_type,
                asset.byte_length as i64,
                asset.filename,
                bytes,
                asset.created_at,
                asset.last_accessed_at,
            ],
        )?;
        Ok(asset)
    }

    fn load_asset(&mut self, id: &str) -> Result<(Asset, Vec<u8>)> {
        let (asset, data): (Asset, Vec<u8>) = self
            .conn
            .query_row(
                "SELECT id, content_type, byte_length, filename, data, created_at, last_accessed_at
                 FROM assets WHERE id = ?1",
                [id],
                |row| {
                    let asset = Asset {
                        id: row.get(0)?,
                        content_type: row.get(1)?,
                        byte_length: row.get::<_, i64>(2)? as usize,
                        filename: row.get(3)?,
                        created_at: row.get(5)?,
                        last_accessed_at: row.get(6)?,
                    };
                    let data = row.get(4)?;
                    Ok((asset, data))
                },
            )
            .optional()?
            .ok_or_else(|| anyhow!("asset not found: {id}"))?;
        self.conn.execute(
            "UPDATE assets SET last_accessed_at = ?2 WHERE id = ?1",
            params![id, now_seconds()],
        )?;
        Ok((asset, data))
    }

    fn touch_session(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET last_active_at = ?2 WHERE id = ?1",
            params![session_id, now_seconds()],
        )?;
        Ok(())
    }

    fn delete_probe_session(&mut self, session_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM clips WHERE session_id = ?1", [session_id])?;
        self.conn
            .execute("DELETE FROM posts WHERE session_id = ?1", [session_id])?;
        self.conn
            .execute("DELETE FROM sessions WHERE id = ?1", [session_id])?;
        Ok(())
    }
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        agent: row.get(1)?,
        title: row.get(2)?,
        cwd: row.get(3)?,
        created_at: row.get(4)?,
        last_active_at: row.get(5)?,
    })
}

fn row_to_post(row: &rusqlite::Row<'_>) -> rusqlite::Result<Post> {
    let surfaces_json: String = row.get(3)?;
    let history_json: String = row.get(7)?;
    Ok(Post {
        id: row.get(0)?,
        session_id: row.get(1)?,
        title: row.get(2)?,
        surfaces: serde_json::from_str(&surfaces_json).map_err(json_error_to_sql)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        version: row.get(6)?,
        history: serde_json::from_str(&history_json).map_err(json_error_to_sql)?,
    })
}

fn row_to_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
    Ok(Asset {
        id: row.get(0)?,
        content_type: row.get(1)?,
        byte_length: row.get::<_, i64>(2)? as usize,
        filename: row.get(3)?,
        created_at: row.get(4)?,
        last_accessed_at: row.get(5)?,
    })
}

fn row_to_clip(row: &rusqlite::Row<'_>) -> rusqlite::Result<Clip> {
    let range_json: Option<String> = row.get(6)?;
    Ok(Clip {
        id: row.get(0)?,
        session_id: row.get(1)?,
        post_id: row.get(2)?,
        post_version: row.get(3)?,
        surface_id: row.get(4)?,
        surface_index: row.get::<_, Option<i64>>(5)?.map(|value| value as usize),
        range: range_json
            .map(|raw| serde_json::from_str(&raw).map_err(json_error_to_sql))
            .transpose()?,
        note: row.get(7)?,
        caption: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn row_to_report(row: &rusqlite::Row<'_>) -> rusqlite::Result<reports::ReportRecord> {
    let meta_json: String = row.get(8)?;
    Ok(reports::ReportRecord {
        id: row.get(0)?,
        kind: row.get(1)?,
        scope_type: row.get(2)?,
        scope_value: row.get(3)?,
        window_start: row.get(4)?,
        window_end: row.get(5)?,
        title: row.get(6)?,
        doc_html: row.get(7)?,
        meta_json: serde_json::from_str(&meta_json).map_err(json_error_to_sql)?,
        generated_at: row.get(9)?,
        requested_by: row.get(10)?,
    })
}

fn json_error_to_sql(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn normalize_surfaces(surfaces: Vec<Surface>) -> Result<Vec<Surface>> {
    if surfaces.is_empty() {
        bail!("at least one surface is required");
    }
    surfaces
        .into_iter()
        .enumerate()
        .map(|(index, surface)| surface.normalize(index))
        .collect()
}

fn normalize_optional_text(input: Option<String>) -> Option<String> {
    input
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_surface_ref(
    surfaces: &[Surface],
    surface_id: Option<&str>,
    surface_index: Option<usize>,
) -> Result<(Option<usize>, Option<Surface>)> {
    if surface_id.is_none() && surface_index.is_none() {
        return Ok((None, None));
    }
    find_surface_ref(surfaces, surface_id, surface_index).ok_or_else(|| {
        anyhow!(
            "surface reference not found: id={:?} index={:?}",
            surface_id,
            surface_index
        )
    })
}

fn find_surface_ref(
    surfaces: &[Surface],
    surface_id: Option<&str>,
    surface_index: Option<usize>,
) -> Option<(Option<usize>, Option<Surface>)> {
    if let Some(index) = surface_index {
        let surface = surfaces.get(index)?;
        if let Some(id) = surface_id
            && surface.id != id
        {
            return None;
        }
        return Some((Some(index), Some(surface.clone())));
    }
    let id = surface_id?;
    surfaces
        .iter()
        .enumerate()
        .find(|(_, surface)| surface.id == id)
        .map(|(index, surface)| (Some(index), Some(surface.clone())))
}

fn post_surfaces_for_version(post: &Post, version: i64) -> Option<&[Surface]> {
    if post.version == version {
        return Some(&post.surfaces);
    }
    post.history
        .iter()
        .find(|entry| entry.version == version)
        .map(|entry| entry.surfaces.as_slice())
}

fn clip_evidence_links(clip: &Clip, surface_index: Option<usize>) -> Vec<ClipEvidenceLink> {
    let mut links = vec![
        ClipEvidenceLink {
            label: "session post".to_string(),
            url: format!("/session/{}/p/{}", clip.session_id, clip.post_id),
        },
        ClipEvidenceLink {
            label: "post json".to_string(),
            url: format!("/api/posts/{}", clip.post_id),
        },
    ];
    if let Some(index) = surface_index {
        links.push(ClipEvidenceLink {
            label: "surface render".to_string(),
            url: format!("/s/{}?part={index}", clip.post_id),
        });
    }
    links
}

fn draft_clip_caption(
    session: &Session,
    post: &Post,
    surface: Option<&Surface>,
    range: Option<&ClipRange>,
    note: Option<&str>,
) -> String {
    // This is the deterministic caption seam. A future model-written caption
    // can replace only this function while policy, storage, and review queue
    // mechanics stay deterministic.
    let mut parts = vec![format!(
        "{} marked \"{}\" v{}",
        session.agent, post.title, post.version
    )];
    if let Some(surface) = surface {
        parts.push(format!("{} surface {}", surface.kind, surface.id));
    }
    if let Some(label) = range.and_then(ClipRange::label) {
        parts.push(label);
    }
    if let Some(note) = note {
        parts.push(note.to_string());
    }
    parts.join(" - ")
}

fn text(s: impl Into<String>) -> Vec<InlineNode> {
    vec![InlineNode::Text { text: s.into() }]
}

fn fresh_id(prefix: &str) -> String {
    let count = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{}-{count}", now_millis())
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub fn app_router(glass: Glass) -> Router {
    Router::new()
        .route("/", get(viewer))
        .route("/favicon.ico", get(favicon))
        .route("/aesthetic.css", get(aesthetic_css))
        .route("/session/{session_id}", get(viewer))
        .route("/session/{session_id}/p/{post_id}", get(viewer))
        .route("/agent/{agent}", get(viewer))
        .route("/clips", get(clips_page))
        .route("/reports", get(reports::reports_shell))
        .route("/reports/{id}", get(reports::report_doc_shell))
        .route("/setup", get(setup))
        .route("/agent-howto", get(agent_howto))
        .route("/api/surface-kinds", get(surface_kinds))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/posts", post(publish_post))
        .route("/api/now", get(now))
        .route("/api/feed/recent", get(recent_feed))
        .route("/api/posts/recent", get(recent_posts))
        .route("/api/posts/{id}", get(get_post).put(update_post))
        .route("/api/clips", get(list_clips).post(capture_clip))
        .route("/api/assets", post(upload_asset))
        .route("/a/{id}", get(serve_asset))
        .route("/s/{post_id}", get(render_sandbox))
        .route("/mcp", post(mcp))
        .route(
            "/api/window-report/{window}",
            get(window_report::window_report),
        )
        .route(
            "/api/reports",
            get(reports::list_reports).post(reports::post_report),
        )
        .route("/rep1", get(reports::redirect_rep1))
        .route("/api/rep1/{window}", get(rep1::rep1_report))
        .route("/review/sample", get(review_report::review_sample_shell))
        .route("/backlog/{repo}", get(reports::redirect_backlog))
        .route("/api/backlog/{repo}", get(backlog_report::backlog_report))
        .route("/needs-you", get(needs_you::needs_you_shell))
        .route("/api/needs-you", get(needs_you::needs_you_report))
        .route("/api/needs-you/answer", post(needs_you::answer))
        .with_state(glass)
        .layer(CatchPanicLayer::custom(canary::panic_response))
}

pub fn start_standing_digest_scheduler(glass: Glass) {
    reports::start_standing_digest_scheduler(glass);
}

async fn viewer() -> Html<String> {
    Html(shell::render_shell(shell::Shell {
        title: "Glass",
        active: Some(shell::Place::Now),
        needs_you_count: needs_you::awaiting_input_count().await,
        sanctum_url: &sanctum_url(),
        styles: VIEWER_STYLE,
        body: VIEWER_BODY,
        scripts: VIEWER_SCRIPT,
    }))
}

/// The cross-repo "back to Sanctum" link (glass-915): config-driven via
/// `GLASS_SANCTUM_URL` rather than a hardcoded personal tailnet hostname, so
/// forks of this public repo don't inherit a link into the origin
/// deployment's infrastructure. Deployments that sit behind a Sanctum portal
/// set the env var to the portal root; unset, the link is inert (`/`).
pub(crate) fn sanctum_url() -> String {
    sanctum_url_from(std::env::var("GLASS_SANCTUM_URL").ok())
}

pub fn sanctum_url_from(configured: Option<String>) -> String {
    configured.unwrap_or_else(|| "/".to_string())
}

async fn favicon() -> StatusCode {
    StatusCode::NO_CONTENT
}

/// The Misty Step Aesthetic kit, vendored at `assets/aesthetic.css` and
/// served as a static stylesheet so both the trusted shell and the
/// sandboxed surface docs can share one set of `--ae-*` tokens and
/// components instead of Glass's own bespoke CSS.
async fn aesthetic_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        AESTHETIC_CSS,
    )
}

async fn setup() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        SETUP_TEXT,
    )
}

async fn agent_howto() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        AGENT_HOWTO,
    )
}

async fn surface_kinds() -> Json<Value> {
    Json(json!({
        "surfaceKinds": SURFACE_KINDS.iter().map(|kind| json!({
            "kind": kind.as_str(),
            "sandboxed": kind.sandboxed(),
            "requiredField": kind.required_field(),
        })).collect::<Vec<_>>()
    }))
}

async fn create_session(
    State(glass): State<Glass>,
    Json(input): Json<NewSession>,
) -> Result<Json<Session>, ApiError> {
    Ok(Json(glass.create_session(input).map_err(|error| {
        api_failure("glass.create_session.failed", "/api/sessions", error)
    })?))
}

async fn list_sessions(State(glass): State<Glass>) -> Result<Json<Value>, ApiError> {
    let sessions = glass
        .list_sessions()
        .map_err(|error| api_failure("glass.list_sessions.failed", "/api/sessions", error))?;
    Ok(Json(json!({ "sessions": sessions })))
}

async fn publish_post(
    State(glass): State<Glass>,
    Json(input): Json<PublishPost>,
) -> Result<Json<PublishOutcome>, ApiError> {
    Ok(Json(glass.publish_post(input).map_err(|error| {
        api_failure("glass.publish_post.failed", "/api/posts", error)
    })?))
}

async fn update_post(
    State(glass): State<Glass>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<PublishPost>,
) -> Result<Json<PublishOutcome>, ApiError> {
    Ok(Json(glass.update_post(&id, input).map_err(|error| {
        api_failure("glass.update_post.failed", "/api/posts/{id}", error)
    })?))
}

async fn get_post(
    State(glass): State<Glass>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Post>, ApiError> {
    Ok(Json(glass.get_post(&id).map_err(|error| {
        api_failure("glass.get_post.failed", "/api/posts/{id}", error)
    })?))
}

#[derive(Debug, Deserialize)]
struct ClipQuery {
    limit: Option<usize>,
}

async fn capture_clip(
    State(glass): State<Glass>,
    Json(input): Json<CaptureClip>,
) -> Result<Json<ClipQueueItem>, ApiError> {
    Ok(Json(glass.capture_clip(input)?))
}

async fn list_clips(
    State(glass): State<Glass>,
    Query(query): Query<ClipQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        json!({ "clips": glass.list_clip_queue(query.limit.unwrap_or(50))? }),
    ))
}

async fn clips_page(State(glass): State<Glass>) -> Result<Html<String>, ApiError> {
    let clips = glass.list_clip_queue(50)?;
    let body = render_clip_queue_body(&clips)?;
    Ok(Html(shell::render_shell(shell::Shell {
        title: "Glass - Clips",
        active: Some(shell::Place::Clips),
        needs_you_count: needs_you::awaiting_input_count().await,
        sanctum_url: &sanctum_url(),
        styles: "",
        body: &body,
        scripts: "",
    })))
}

#[derive(Debug, Deserialize)]
struct RecentQuery {
    limit: Option<usize>,
    /// Restrict `posts` to one agent's status feed (`/agent/:agent`).
    /// `sessions`/`agents` stay unfiltered so the rail always shows the
    /// whole fleet regardless of which feed is open.
    agent: Option<String>,
    /// Restrict `posts` to one session (`/session/:id`).
    #[serde(alias = "sessionId")]
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FeedQuery {
    limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FeedEvent {
    id: String,
    kind: FeedKind,
    source: String,
    title: String,
    summary: String,
    occurred_at: i64,
    agent: Option<String>,
    session_id: Option<String>,
    session_title: Option<String>,
    post_id: Option<String>,
    evidence_links: Vec<EvidenceLink>,
    detail_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LandmarkStatus {
    status: &'static str,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NowResponse {
    stats: NowStats,
    wall: Vec<NowWallCard>,
    wire: Vec<FeedEvent>,
    dead: NowDeadSessions,
    notices: Vec<NowNotice>,
    landmark: LandmarkStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NowStats {
    agents_live: usize,
    need_you_count: Option<usize>,
    posts_today: usize,
    sessions_today: usize,
    seconds_since_last_event: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NowNotice {
    kind: &'static str,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NowDeadSessions {
    agent_count: usize,
    session_count: usize,
    sessions: Vec<NowDeadSession>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NowDeadSession {
    agent: String,
    title: String,
    href: String,
    last_active_at: i64,
    age_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NowWallCard {
    agent: String,
    href: String,
    status: &'static str,
    powder_tag: Option<String>,
    powder_card_id: Option<String>,
    powder_title: Option<String>,
    meta: String,
    session_id: Option<String>,
    session_title: Option<String>,
    post_id: Option<String>,
    latest_kind: Option<FeedKind>,
    latest_at: Option<i64>,
    age_seconds: Option<i64>,
    claimed_at: Option<i64>,
    quiet: bool,
    trace: Vec<NowTraceItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NowTraceItem {
    kind: FeedKind,
    at: i64,
}

#[derive(Debug, Clone)]
struct PowderClaim {
    id: String,
    title: String,
    agent: String,
    acquired_at: Option<i64>,
    updated_at: i64,
}

async fn now(State(glass): State<Glass>) -> Result<Json<NowResponse>, ApiError> {
    let raw_posts = glass
        .list_recent_posts(100)
        .map_err(|error| api_failure("glass.now.failed", "/api/now", error))?;
    let raw_sessions = glass
        .list_sessions()
        .map_err(|error| api_failure("glass.now.failed", "/api/now", error))?;
    let raw_session_by_id = raw_sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect::<HashMap<_, _>>();
    let posts = raw_posts
        .into_iter()
        .filter(|post| {
            raw_session_by_id
                .get(post.session_id.as_str())
                .is_none_or(|session| !is_diagnostic_agent(&session.agent))
        })
        .collect::<Vec<_>>();
    let sessions = raw_sessions
        .into_iter()
        .filter(|session| !is_diagnostic_agent(&session.agent))
        .collect::<Vec<_>>();
    let session_by_id = sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect::<HashMap<_, _>>();

    let mut wire = posts
        .iter()
        .map(|post| post_feed_event(post, session_by_id.get(post.session_id.as_str()).copied()))
        .collect::<Vec<_>>();
    let landmark = fetch_landmark_feed(40).await;
    wire.extend(landmark.events.clone());
    wire.sort_by(|left, right| {
        right
            .occurred_at
            .cmp(&left.occurred_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    wire.truncate(40);

    let mut notices = Vec::new();
    let claims = match fetch_powder_active_claims().await {
        Ok(claims) => claims,
        Err(message) => {
            notices.push(NowNotice {
                kind: "powder",
                message: format!("Powder active claims unavailable: {message}"),
            });
            Vec::new()
        }
    };

    let now = now_seconds();
    let wall = build_now_wall(&claims, &posts, &sessions, now);
    let dead = build_now_dead_sessions(&posts, &sessions, now);
    let day_start = utc_day_start(now);
    let stats = NowStats {
        agents_live: wall.len(),
        need_you_count: needs_you::awaiting_input_count().await,
        posts_today: posts
            .iter()
            .filter(|post| post.created_at >= day_start || post.updated_at >= day_start)
            .count(),
        sessions_today: sessions
            .iter()
            .filter(|session| session.created_at >= day_start)
            .count(),
        seconds_since_last_event: wire
            .first()
            .map(|event| now.saturating_sub(event.occurred_at)),
    };

    Ok(Json(NowResponse {
        stats,
        wall,
        wire,
        dead,
        notices,
        landmark: landmark.status,
    }))
}

async fn recent_posts(
    State(glass): State<Glass>,
    Query(query): Query<RecentQuery>,
) -> Result<Json<Value>, ApiError> {
    let fetch_limit = if query.agent.is_some() || query.session_id.is_some() {
        100
    } else {
        query.limit.unwrap_or(30)
    };
    let raw_posts = glass
        .list_recent_posts(fetch_limit)
        .map_err(|error| api_failure("glass.recent_posts.failed", "/api/posts/recent", error))?;
    let raw_sessions = glass
        .list_sessions()
        .map_err(|error| api_failure("glass.recent_posts.failed", "/api/posts/recent", error))?;
    let raw_session_by_id = raw_sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect::<HashMap<_, _>>();
    let posts = raw_posts
        .into_iter()
        .filter(|post| {
            raw_session_by_id
                .get(post.session_id.as_str())
                .is_none_or(|session| !is_diagnostic_agent(&session.agent))
        })
        .collect::<Vec<_>>();
    let sessions = raw_sessions
        .into_iter()
        .filter(|session| !is_diagnostic_agent(&session.agent))
        .collect::<Vec<_>>();
    let session_by_id = sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect::<HashMap<_, _>>();
    let agents = summarize_agents(&posts, &sessions);
    let view_posts = posts
        .into_iter()
        .filter(|post| {
            let session_ok = query
                .session_id
                .as_deref()
                .is_none_or(|id| post.session_id == id);
            let agent_ok = query.agent.as_deref().is_none_or(|agent| {
                session_by_id
                    .get(post.session_id.as_str())
                    .is_some_and(|session| session.agent == agent)
            });
            session_ok && agent_ok
        })
        .collect::<Vec<_>>();
    let now = now_seconds();
    let session_views = sessions
        .iter()
        .map(|session| SessionView {
            session,
            is_live: now - session.last_active_at < LIVE_WINDOW_SECONDS,
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "posts": view_posts,
        "sessions": session_views,
        "agents": agents,
    })))
}

async fn recent_feed(
    State(glass): State<Glass>,
    Query(query): Query<FeedQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(80).clamp(1, 100);
    let raw_posts = glass.list_recent_posts(limit)?;
    let raw_sessions = glass.list_sessions()?;
    let raw_session_by_id = raw_sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect::<HashMap<_, _>>();
    let posts = raw_posts
        .into_iter()
        .filter(|post| {
            raw_session_by_id
                .get(post.session_id.as_str())
                .is_none_or(|session| !is_diagnostic_agent(&session.agent))
        })
        .collect::<Vec<_>>();
    let sessions = raw_sessions
        .into_iter()
        .filter(|session| !is_diagnostic_agent(&session.agent))
        .collect::<Vec<_>>();
    let session_by_id = sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect::<HashMap<_, _>>();
    let agents = summarize_agents(&posts, &sessions);
    let now = now_seconds();
    let session_views = sessions
        .iter()
        .map(|session| SessionView {
            session,
            is_live: now - session.last_active_at < LIVE_WINDOW_SECONDS,
        })
        .collect::<Vec<_>>();

    let mut events = posts
        .iter()
        .map(|post| post_feed_event(post, session_by_id.get(post.session_id.as_str()).copied()))
        .collect::<Vec<_>>();
    let landmark = fetch_landmark_feed(limit).await;
    events.extend(landmark.events);
    events.sort_by(|left, right| {
        right
            .occurred_at
            .cmp(&left.occurred_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    events.truncate(limit);

    Ok(Json(json!({
        "events": events,
        "posts": posts,
        "sessions": session_views,
        "agents": agents,
        "feedKinds": FEED_KINDS,
        "landmark": landmark.status,
    })))
}

/// A session as the operator stream reports it: the stored fields plus
/// whether it still counts as live for the primary rail (see
/// `LIVE_WINDOW_SECONDS`). Dead sessions keep their full post history on
/// their own agent feed; they just stop appearing as fleet-wall peers.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionView<'a> {
    #[serde(flatten)]
    session: &'a Session,
    is_live: bool,
}

/// Diagnostic agents (the doctor's own probes) prove the live round trip but
/// are not operator content; the operator-facing stream excludes them.
fn is_diagnostic_agent(agent: &str) -> bool {
    agent == "glass-doctor"
}

fn summarize_agents(posts: &[Post], sessions: &[Session]) -> Vec<AgentSummary> {
    #[derive(Default)]
    struct AgentAccumulator {
        post_count: usize,
        session_ids: BTreeSet<String>,
        last_active_at: i64,
    }

    let session_by_id = sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect::<HashMap<_, _>>();
    let mut by_agent = BTreeMap::<String, AgentAccumulator>::new();
    for post in posts {
        let Some(session) = session_by_id.get(post.session_id.as_str()) else {
            continue;
        };
        let entry = by_agent.entry(session.agent.clone()).or_default();
        entry.post_count += 1;
        entry.session_ids.insert(session.id.clone());
        entry.last_active_at = entry
            .last_active_at
            .max(session.last_active_at)
            .max(post.updated_at);
    }
    let mut agents = by_agent
        .into_iter()
        .map(|(agent, entry)| AgentSummary {
            agent,
            post_count: entry.post_count,
            session_count: entry.session_ids.len(),
            last_active_at: entry.last_active_at,
        })
        .collect::<Vec<_>>();
    agents.sort_by(|left, right| {
        right
            .last_active_at
            .cmp(&left.last_active_at)
            .then_with(|| left.agent.cmp(&right.agent))
    });
    agents
}

fn build_now_wall(
    claims: &[PowderClaim],
    posts: &[Post],
    sessions: &[Session],
    now: i64,
) -> Vec<NowWallCard> {
    let session_by_id = sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect::<HashMap<_, _>>();
    let mut latest_by_agent: BTreeMap<String, (&Session, &Post)> = BTreeMap::new();
    for post in posts {
        let Some(session) = session_by_id.get(post.session_id.as_str()) else {
            continue;
        };
        if !session_is_live(session, now) {
            continue;
        }
        let event_at = post.updated_at.max(post.created_at);
        let replace = latest_by_agent
            .get(&session.agent)
            .is_none_or(|(_, current)| event_at > current.updated_at.max(current.created_at));
        if replace {
            latest_by_agent.insert(session.agent.clone(), (session, post));
        }
    }

    let mut claims_by_agent: BTreeMap<String, &PowderClaim> = BTreeMap::new();
    for claim in claims {
        let replace = claims_by_agent.get(&claim.agent).is_none_or(|current| {
            claim_sort_at(claim) > claim_sort_at(current)
                || (claim_sort_at(claim) == claim_sort_at(current) && claim.id < current.id)
        });
        if replace {
            claims_by_agent.insert(claim.agent.clone(), claim);
        }
    }

    let mut agents = BTreeSet::new();
    agents.extend(latest_by_agent.keys().cloned());
    agents.extend(claims_by_agent.keys().cloned());
    let mut cards = agents
        .into_iter()
        .map(|agent| {
            let latest = latest_by_agent.get(&agent).copied();
            let claim = claims_by_agent.get(&agent).copied();
            now_wall_card(agent, latest, claim, posts, &session_by_id, now)
        })
        .collect::<Vec<_>>();
    cards.sort_by(|left, right| {
        wall_sort_at(right)
            .cmp(&wall_sort_at(left))
            .then_with(|| left.agent.cmp(&right.agent))
    });
    cards
}

fn now_wall_card(
    agent: String,
    latest: Option<(&Session, &Post)>,
    claim: Option<&PowderClaim>,
    posts: &[Post],
    session_by_id: &HashMap<&str, &Session>,
    now: i64,
) -> NowWallCard {
    let href = format!("/agent/{}", url_path_segment(&agent));
    if let Some((session, post)) = latest {
        let latest_kind = feed_kind_for_post(post);
        let latest_at = post.updated_at.max(post.created_at);
        return NowWallCard {
            agent: agent.clone(),
            href,
            status: if latest_kind == FeedKind::Blocked {
                "warn"
            } else {
                "ok"
            },
            powder_tag: claim.map(|claim| format!("powder {}", claim.id)),
            powder_card_id: claim.map(|claim| claim.id.clone()),
            powder_title: claim.map(|claim| claim.title.clone()),
            meta: latest_declared_act(post),
            session_id: Some(session.id.clone()),
            session_title: Some(session.title.clone()),
            post_id: Some(post.id.clone()),
            latest_kind: Some(latest_kind),
            latest_at: Some(latest_at),
            age_seconds: Some(now.saturating_sub(latest_at)),
            claimed_at: claim.and_then(|claim| claim.acquired_at),
            quiet: false,
            trace: trace_for_agent(&agent, posts, session_by_id),
        };
    }

    let claim = claim.expect("wall agent without live posts must have a claim");
    let claimed_at = claim.acquired_at.unwrap_or(claim.updated_at);
    NowWallCard {
        agent,
        href,
        status: "quiet",
        powder_tag: Some(format!("powder {}", claim.id)),
        powder_card_id: Some(claim.id.clone()),
        powder_title: Some(claim.title.clone()),
        meta: format!(
            "claimed {} ago · no posts yet",
            compact_age(now.saturating_sub(claimed_at))
        ),
        session_id: None,
        session_title: None,
        post_id: None,
        latest_kind: None,
        latest_at: None,
        age_seconds: Some(now.saturating_sub(claimed_at)),
        claimed_at: Some(claimed_at),
        quiet: true,
        trace: Vec::new(),
    }
}

fn latest_declared_act(post: &Post) -> String {
    let kind = feed_kind_for_post(post);
    let act = declared_summary(post)
        .filter(|summary| !summary.trim().is_empty())
        .unwrap_or_else(|| post.title.clone());
    format!("{kind}: {act}")
}

fn trace_for_agent(
    agent: &str,
    posts: &[Post],
    session_by_id: &HashMap<&str, &Session>,
) -> Vec<NowTraceItem> {
    posts
        .iter()
        .filter(|post| {
            session_by_id
                .get(post.session_id.as_str())
                .is_some_and(|session| session.agent == agent)
        })
        .take(4)
        .map(|post| NowTraceItem {
            kind: feed_kind_for_post(post),
            at: post.updated_at.max(post.created_at),
        })
        .collect()
}

fn build_now_dead_sessions(posts: &[Post], sessions: &[Session], now: i64) -> NowDeadSessions {
    let latest_post_by_session =
        posts
            .iter()
            .fold(HashMap::<&str, &Post>::new(), |mut acc, post| {
                acc.entry(post.session_id.as_str()).or_insert(post);
                acc
            });
    let day_ago = now.saturating_sub(86_400);
    let mut sessions = sessions
        .iter()
        .filter(|session| {
            !session_is_live(session, now)
                && session.last_active_at >= day_ago
                && latest_post_by_session.contains_key(session.id.as_str())
        })
        .map(|session| NowDeadSession {
            agent: session.agent.clone(),
            title: session.title.clone(),
            href: format!("/agent/{}", url_path_segment(&session.agent)),
            last_active_at: session.last_active_at,
            age_seconds: now.saturating_sub(session.last_active_at),
        })
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        right
            .last_active_at
            .cmp(&left.last_active_at)
            .then_with(|| left.agent.cmp(&right.agent))
    });
    let agent_count = sessions
        .iter()
        .map(|session| session.agent.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let session_count = sessions.len();
    sessions.truncate(12);
    NowDeadSessions {
        agent_count,
        session_count,
        sessions,
    }
}

fn session_is_live(session: &Session, now: i64) -> bool {
    now.saturating_sub(session.last_active_at) < LIVE_WINDOW_SECONDS
}

fn wall_sort_at(card: &NowWallCard) -> i64 {
    card.latest_at.or(card.claimed_at).unwrap_or_default()
}

fn claim_sort_at(claim: &PowderClaim) -> i64 {
    claim.acquired_at.unwrap_or(claim.updated_at)
}

fn utc_day_start(now: i64) -> i64 {
    now - now.rem_euclid(86_400)
}

fn compact_age(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86_400)
    }
}

fn url_path_segment(raw: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
    for byte in raw.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(*byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

async fn fetch_powder_active_claims() -> Result<Vec<PowderClaim>, String> {
    let base = std::env::var("GLASS_POWDER_API_BASE_URL")
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "GLASS_POWDER_API_BASE_URL is not configured".to_string())?;
    let key = std::env::var("GLASS_POWDER_API_KEY")
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "GLASS_POWDER_API_KEY is not configured".to_string())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(900))
        .build()
        .map_err(|err| format!("build Powder client: {err}"))?;
    let mut claims_by_id = BTreeMap::<String, PowderClaim>::new();
    for status in ["claimed", "running", "awaiting_input"] {
        let url = format!(
            "{}/api/v1/cards?status={status}&limit=100",
            base.trim_end_matches('/')
        );
        let response = client
            .get(&url)
            .bearer_auth(&key)
            .send()
            .await
            .map_err(|err| {
                canary::report_error(
                    "glass.now.powder.failed",
                    "route=/api/now upstream=powder error_kind=transport",
                );
                format!("fetch active {status} cards: {err}")
            })?;
        if !response.status().is_success() {
            canary::report_error(
                "glass.now.powder.failed",
                &format!(
                    "route=/api/now upstream=powder upstream_status={} error_kind=upstream_status",
                    response.status().as_u16()
                ),
            );
            return Err(format!(
                "fetch active {status} cards: upstream returned {}",
                response.status()
            ));
        }
        let value = response.json::<Value>().await.map_err(|err| {
            canary::report_error(
                "glass.now.powder.failed",
                "route=/api/now upstream=powder error_kind=parse",
            );
            format!("parse active {status} cards: {err}")
        })?;
        let cards = value
            .get("cards")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("parse active {status} cards: missing cards array"))?;
        for card in cards {
            if let Some(claim) = powder_claim_from_card(card) {
                claims_by_id.entry(claim.id.clone()).or_insert(claim);
            }
        }
    }
    let mut claims = claims_by_id.into_values().collect::<Vec<_>>();
    claims.sort_by(|left, right| {
        claim_sort_at(right)
            .cmp(&claim_sort_at(left))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(claims)
}

fn powder_claim_from_card(value: &Value) -> Option<PowderClaim> {
    let object = value.as_object()?;
    let claim = object.get("claim").and_then(Value::as_object)?;
    let agent = string_field(claim, &["agent", "assignee"])
        .map(trimmed_owned)
        .filter(|agent| !agent.is_empty())?;
    let id = string_field(object, &["id", "card_id"])
        .map(trimmed_owned)
        .filter(|id| !id.is_empty())?;
    let title = string_field(object, &["title", "name"])
        .map(trimmed_owned)
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| id.clone());
    let acquired_at = timestamp_field(claim, &["acquired_at", "acquiredAt", "created_at"]);
    let updated_at = timestamp_field(object, &["updated_at", "updatedAt", "created_at"])
        .or_else(|| timestamp_field(claim, &["updated_at", "updatedAt", "expires_at"]))
        .unwrap_or_else(now_seconds);
    Some(PowderClaim {
        id,
        title,
        agent,
        acquired_at,
        updated_at,
    })
}

fn render_clip_queue_body(clips: &[ClipQueueItem]) -> Result<String> {
    let hero = Component::Hero(Hero {
        title: "Clip review queue".to_string(),
        summary: text("Marked live-stage moments, packaged with post context and draft captions."),
        stats: vec![Metric {
            label: "Queued clips".to_string(),
            value: clips.len().to_string(),
        }],
        image_intent: None,
    });
    let columns = vec![
        ColumnSpec {
            key: "created".to_string(),
            label: "Created".to_string(),
            numeric: true,
            emphasize: false,
        },
        ColumnSpec {
            key: "caption".to_string(),
            label: "Draft caption".to_string(),
            numeric: false,
            emphasize: true,
        },
        ColumnSpec {
            key: "surface".to_string(),
            label: "Surface".to_string(),
            numeric: false,
            emphasize: false,
        },
        ColumnSpec {
            key: "evidence".to_string(),
            label: "Evidence".to_string(),
            numeric: false,
            emphasize: false,
        },
        ColumnSpec {
            key: "note".to_string(),
            label: "Note".to_string(),
            numeric: false,
            emphasize: false,
        },
    ];
    let rows = clips
        .iter()
        .map(|item| {
            let surface_label = item
                .context
                .surface
                .as_ref()
                .map(|surface| format!("{} / {}", surface.kind, surface.id))
                .unwrap_or_else(|| "whole post".to_string());
            let evidence = item
                .context
                .evidence_links
                .first()
                .cloned()
                .unwrap_or_else(|| ClipEvidenceLink {
                    label: "post".to_string(),
                    url: format!("/session/{}/p/{}", item.clip.session_id, item.clip.post_id),
                });
            Row {
                cells: vec![
                    Cell {
                        column_key: "created".to_string(),
                        value: CellValue::Text {
                            text: item.clip.created_at.to_string(),
                        },
                    },
                    Cell {
                        column_key: "caption".to_string(),
                        value: CellValue::Text {
                            text: item.draft_caption.clone(),
                        },
                    },
                    Cell {
                        column_key: "surface".to_string(),
                        value: CellValue::Text {
                            text: surface_label,
                        },
                    },
                    Cell {
                        column_key: "evidence".to_string(),
                        value: CellValue::Link {
                            text: evidence.label,
                            href: evidence.url,
                        },
                    },
                    Cell {
                        column_key: "note".to_string(),
                        value: CellValue::Text {
                            text: item.clip.note.clone().unwrap_or_else(|| "-".to_string()),
                        },
                    },
                ],
            }
        })
        .collect::<Vec<_>>();
    let table = Component::Table(Table {
        heading: "Review candidates".to_string(),
        columns,
        rows,
        empty_note: (clips.is_empty()).then(|| {
            "No clips captured yet. Capture with MCP capture_clip or POST /api/clips.".to_string()
        }),
        demoted_note: None,
    });
    let components = vec![hero, table];
    validate_layout(&components, &REPORT).map_err(|error| anyhow!(error.to_string()))?;
    let ctx = RenderContext {
        now: Utc::now(),
        cite_href: &|ref_id| format!("#cite-{ref_id}"),
    };
    Ok(components
        .iter()
        .map(|component| render_component(component, &ctx))
        .collect())
}

fn post_feed_event(post: &Post, session: Option<&Session>) -> FeedEvent {
    let kind = feed_kind_for_post(post);
    let evidence_links = evidence_links_for_post(post);
    let surface_kinds = post
        .surfaces
        .iter()
        .map(|surface| surface.kind.as_str())
        .collect::<Vec<_>>();
    let summary = declared_summary(post).unwrap_or_else(|| {
        format!(
            "{} surface(s): {}",
            post.surfaces.len(),
            surface_kinds.join(", ")
        )
    });
    let mut detail_lines = Vec::new();
    if let Some(session) = session {
        detail_lines.push(format!("agent: {}", session.agent));
        detail_lines.push(format!("session: {}", session.title));
    }
    detail_lines.push(format!("post: {}", post.id));
    detail_lines.push(format!("version: {}", post.version));
    detail_lines.push(format!("surfaces: {}", surface_kinds.join(", ")));
    if let Some(detail) = declared_detail(post) {
        detail_lines.push(detail);
    }

    FeedEvent {
        id: format!("post:{}", post.id),
        kind,
        source: "glass-posts".to_string(),
        title: post.title.clone(),
        summary,
        occurred_at: post.updated_at.max(post.created_at),
        agent: session.map(|session| session.agent.clone()),
        session_id: Some(post.session_id.clone()),
        session_title: session.map(|session| session.title.clone()),
        post_id: Some(post.id.clone()),
        evidence_links,
        detail_lines,
    }
}

pub(crate) fn feed_kind_for_post(post: &Post) -> FeedKind {
    post.surfaces
        .iter()
        .find_map(feed_kind_from_surface)
        .unwrap_or_default()
}

fn feed_kind_from_surface(surface: &Surface) -> Option<FeedKind> {
    string_field(&surface.fields, &["feedKind", "feed_kind", "feed"])
        .or_else(|| {
            nested_string_field(&surface.fields, "data", &["feedKind", "feed_kind", "kind"])
        })
        .and_then(|raw| FeedKind::from_str(raw).ok())
}

pub(crate) fn declared_summary(post: &Post) -> Option<String> {
    post.surfaces.iter().find_map(|surface| {
        string_field(&surface.fields, &["summary", "feedSummary", "feed_summary"])
            .or_else(|| nested_string_field(&surface.fields, "data", &["summary", "feedSummary"]))
            .map(trimmed_owned)
            .filter(|value| !value.is_empty())
    })
}

fn declared_detail(post: &Post) -> Option<String> {
    post.surfaces.iter().find_map(|surface| {
        string_field(&surface.fields, &["detail", "body", "markdown"])
            .or_else(|| {
                nested_string_field(&surface.fields, "data", &["detail", "body", "markdown"])
            })
            .map(trimmed_owned)
            .filter(|value| !value.is_empty())
    })
}

pub(crate) fn evidence_links_for_post(post: &Post) -> Vec<EvidenceLink> {
    let mut links = Vec::new();
    for surface in &post.surfaces {
        links.extend(evidence_links_from_surface(surface));
    }
    links.push(EvidenceLink {
        label: "post detail".to_string(),
        url: format!("/session/{}/p/{}", post.session_id, post.id),
    });
    for (index, surface) in post.surfaces.iter().enumerate() {
        let label = format!(
            "{} {}",
            surface.kind,
            if surface.id.is_empty() {
                (index + 1).to_string()
            } else {
                surface.id.clone()
            }
        );
        if surface.kind.sandboxed() {
            links.push(EvidenceLink {
                label,
                url: format!("/s/{}?part={index}", post.id),
            });
        } else if surface.kind == SurfaceKind::Image
            && let Some(asset_id) = surface.fields.get("asset_id").and_then(Value::as_str)
        {
            links.push(EvidenceLink {
                label,
                url: format!("/a/{asset_id}"),
            });
        }
    }
    dedupe_evidence_links(links)
}

fn evidence_links_from_surface(surface: &Surface) -> Vec<EvidenceLink> {
    let mut links = Vec::new();
    for key in ["evidenceLinks", "evidence_links", "links"] {
        if let Some(value) = surface.fields.get(key) {
            links.extend(evidence_links_from_value(value));
        }
    }
    if let Some(data) = surface.fields.get("data").and_then(Value::as_object) {
        for key in ["evidenceLinks", "evidence_links", "links"] {
            if let Some(value) = data.get(key) {
                links.extend(evidence_links_from_value(value));
            }
        }
    }
    links
}

fn evidence_links_from_value(value: &Value) -> Vec<EvidenceLink> {
    match value {
        Value::Array(items) => items.iter().filter_map(evidence_link_from_value).collect(),
        Value::String(url) => evidence_link_from_parts(None, url),
        Value::Object(_) => evidence_link_from_value(value).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn evidence_link_from_value(value: &Value) -> Option<EvidenceLink> {
    match value {
        Value::String(url) => evidence_link_from_parts(None, url).into_iter().next(),
        Value::Object(object) => {
            let url = string_field(object, &["url", "href", "link", "html_url"])?;
            let label = string_field(object, &["label", "title", "text"]);
            evidence_link_from_parts(label, url).into_iter().next()
        }
        _ => None,
    }
}

fn evidence_link_from_parts(label: Option<&str>, url: &str) -> Vec<EvidenceLink> {
    let url = url.trim();
    if !safe_href(url) {
        return Vec::new();
    }
    vec![EvidenceLink {
        label: label.map_or_else(|| default_link_label(url), trimmed_owned),
        url: url.to_string(),
    }]
}

fn dedupe_evidence_links(links: Vec<EvidenceLink>) -> Vec<EvidenceLink> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for link in links {
        if link.url.is_empty() || !seen.insert(link.url.clone()) {
            continue;
        }
        out.push(link);
    }
    out
}

fn safe_href(url: &str) -> bool {
    url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with('/')
        || url.starts_with('#')
}

fn default_link_label(url: &str) -> String {
    let after_scheme = url.split_once("//").map_or(url, |(_, rest)| rest);
    truncate_chars(after_scheme.trim_start_matches('/'), 44)
}

fn string_field<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
}

fn nested_string_field<'a>(
    object: &'a Map<String, Value>,
    nested_key: &str,
    keys: &[&str],
) -> Option<&'a str> {
    object
        .get(nested_key)
        .and_then(Value::as_object)
        .and_then(|nested| string_field(nested, keys))
}

fn trimmed_owned(raw: &str) -> String {
    raw.trim().to_string()
}

fn truncate_chars(raw: &str, limit: usize) -> String {
    if raw.chars().count() <= limit {
        return raw.to_string();
    }
    let suffix = "...";
    let keep = limit.saturating_sub(suffix.len());
    let mut out = raw.chars().take(keep).collect::<String>();
    out.push_str(suffix);
    out
}

struct LandmarkFeed {
    events: Vec<FeedEvent>,
    status: LandmarkStatus,
}

fn landmark_release_events_url() -> Option<String> {
    std::env::var("GLASS_LANDMARK_RELEASE_EVENTS_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

async fn fetch_landmark_feed(limit: usize) -> LandmarkFeed {
    let Some(url) = landmark_release_events_url() else {
        return LandmarkFeed {
            events: Vec::new(),
            status: LandmarkStatus {
                status: "unconfigured",
                message: Some("GLASS_LANDMARK_RELEASE_EVENTS_URL is not configured".to_string()),
            },
        };
    };
    match fetch_landmark_release_events(&url, limit).await {
        Ok(events) => LandmarkFeed {
            events,
            status: LandmarkStatus {
                status: "ok",
                message: None,
            },
        },
        Err(message) => LandmarkFeed {
            events: Vec::new(),
            status: LandmarkStatus {
                status: "error",
                message: Some(message),
            },
        },
    }
}

async fn fetch_landmark_release_events(url: &str, limit: usize) -> Result<Vec<FeedEvent>, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|err| format!("fetch {url}: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "fetch {url}: upstream returned {}",
            response.status()
        ));
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|err| format!("parse {url}: {err}"))?;
    Ok(landmark_release_events_from_value(&value, limit))
}

fn landmark_release_events_from_value(value: &Value, limit: usize) -> Vec<FeedEvent> {
    let Some(items) = value
        .as_array()
        .or_else(|| value.get("events").and_then(Value::as_array))
        .or_else(|| value.get("releases").and_then(Value::as_array))
    else {
        return Vec::new();
    };
    let mut events = items
        .iter()
        .filter_map(landmark_release_event_from_value)
        .collect::<Vec<_>>();
    events.sort_by_key(|event| std::cmp::Reverse(event.occurred_at));
    events.truncate(limit);
    events
}

fn landmark_release_event_from_value(value: &Value) -> Option<FeedEvent> {
    let object = value.as_object()?;
    let repo = string_field(object, &["repo", "repository", "project"]).unwrap_or("landmark");
    let version = string_field(object, &["version", "tag", "release"]).unwrap_or("release");
    let title = string_field(object, &["title", "name"])
        .map(trimmed_owned)
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| format!("{repo} {version} released"));
    let summary = string_field(object, &["summary", "body", "notes"])
        .map(|raw| {
            raw.lines()
                .find(|line| !line.trim().is_empty())
                .map_or_else(|| title.clone(), |line| truncate_chars(line.trim(), 180))
        })
        .unwrap_or_else(|| format!("Landmark release event for {repo} {version}."));
    let occurred_at = timestamp_field(object, &["created_at", "published_at", "timestamp", "at"])
        .unwrap_or_else(now_seconds);
    let mut evidence_links = Vec::new();
    for key in ["evidenceLinks", "evidence_links", "links"] {
        if let Some(value) = object.get(key) {
            evidence_links.extend(evidence_links_from_value(value));
        }
    }
    if let Some(url) = string_field(object, &["url", "html_url"]) {
        evidence_links.extend(evidence_link_from_parts(Some("release"), url));
    }
    let id = string_field(object, &["id", "event_id"])
        .map(trimmed_owned)
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| format!("landmark:{repo}:{version}:{occurred_at}"));
    let detail_lines = vec![
        format!("repo: {repo}"),
        format!("version: {version}"),
        "source: Landmark release events".to_string(),
    ];

    Some(FeedEvent {
        id,
        kind: FeedKind::Release,
        source: "landmark".to_string(),
        title,
        summary,
        occurred_at,
        agent: Some("landmark".to_string()),
        session_id: None,
        session_title: None,
        post_id: None,
        evidence_links: dedupe_evidence_links(evidence_links),
        detail_lines,
    })
}

fn timestamp_field(object: &Map<String, Value>, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        object.get(*key).and_then(|value| {
            value.as_i64().or_else(|| {
                value
                    .as_str()
                    .and_then(|raw| chrono::DateTime::parse_from_rfc3339(raw).ok())
                    .map(|dt| dt.timestamp())
            })
        })
    })
}

async fn upload_asset(
    State(glass): State<Glass>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream");
    let filename = headers
        .get("x-filename")
        .and_then(|value| value.to_str().ok());
    let asset = glass
        .store_asset(content_type, filename, &body)
        .map_err(|error| api_failure("glass.upload_asset.failed", "/api/assets", error))?;
    Ok(Json(
        json!({ "asset": asset, "url": format!("/a/{}", asset.id) }),
    ))
}

async fn serve_asset(
    State(glass): State<Glass>,
    AxumPath(id): AxumPath<String>,
) -> Result<Response, ApiError> {
    let (asset, data) = glass
        .load_asset(&id)
        .map_err(|error| api_failure("glass.serve_asset.failed", "/a/{id}", error))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&asset.content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    if !asset.content_type.starts_with("image/") {
        headers.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment"),
        );
    }
    Ok((headers, data).into_response())
}

#[derive(Debug, Deserialize)]
struct RenderQuery {
    part: Option<usize>,
}

async fn render_sandbox(
    State(glass): State<Glass>,
    AxumPath(post_id): AxumPath<String>,
    Query(query): Query<RenderQuery>,
) -> Result<Response, ApiError> {
    let post = glass
        .get_post(&post_id)
        .map_err(|error| api_failure("glass.render_sandbox.failed", "/s/{post_id}", error))?;
    let part = query.part.unwrap_or(0);
    let surface = post
        .surfaces
        .get(part)
        .ok_or_else(|| ApiError::not_found(format!("surface part not found: {part}")))?;
    let html = render_surface_doc(&post, surface);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(SANDBOX_CSP),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    Ok((headers, html).into_response())
}

async fn mcp(State(glass): State<Glass>, Json(request): Json<Value>) -> Json<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    match mcp_dispatch(&glass, &request) {
        Ok(result) => Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })),
        Err(error) => {
            canary::report_error("glass.mcp.failed", "route=/mcp error_kind=dispatch");
            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32602, "message": error.to_string() }
            }))
        }
    }
}

fn mcp_dispatch(glass: &Glass, request: &Value) -> Result<Value> {
    match request.get("method").and_then(Value::as_str) {
        Some("initialize") => Ok(json!({
            "protocolVersion": "2025-06-18",
            "serverInfo": { "name": "glass", "version": env!("CARGO_PKG_VERSION") },
            "capabilities": { "tools": {} }
        })),
        Some("tools/list") => Ok(json!({ "tools": mcp_tools() })),
        Some("tools/call") => {
            let params = request.get("params").cloned().unwrap_or_default();
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match name {
                "publish_post" => serde_json::from_value::<PublishPost>(args)
                    .map_err(|error| anyhow!(error))
                    .and_then(|input| glass.publish_post(input))
                    .map(|outcome| json!({ "content": [{ "type": "json", "json": outcome }] })),
                "capture_clip" => serde_json::from_value::<CaptureClip>(args)
                    .map_err(|error| anyhow!(error))
                    .and_then(|input| glass.capture_clip(input))
                    .map(|item| json!({ "content": [{ "type": "json", "json": item }] })),
                _ => Err(anyhow!("unknown tool: {name}")),
            }
        }
        _ => Err(anyhow!("unsupported JSON-RPC method")),
    }
}

fn mcp_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "publish_post",
            "description": "Publish or create a Glass post made from ordered typed surfaces.",
            "inputSchema": {
                "type": "object",
                "required": ["title", "surfaces"],
                "properties": {
                    "session_id": { "type": "string" },
                    "session_title": { "type": "string" },
                    "agent": { "type": "string" },
                    "title": { "type": "string" },
                    "surfaces": { "type": "array" }
                }
            }
        }),
        json!({
            "name": "capture_clip",
            "description": "Mark an interesting Glass post or surface for the one-way clip review queue.",
            "inputSchema": {
                "type": "object",
                "required": ["session_id", "post_id"],
                "properties": {
                    "session_id": { "type": "string" },
                    "post_id": { "type": "string" },
                    "surface_id": { "type": "string" },
                    "surface_index": { "type": "integer", "minimum": 0 },
                    "range": {
                        "type": "object",
                        "properties": {
                            "start": { "type": "integer", "minimum": 0 },
                            "end": { "type": "integer", "minimum": 0 }
                        }
                    },
                    "note": { "type": "string" }
                }
            }
        }),
    ]
}

fn render_surface_doc(post: &Post, surface: &Surface) -> String {
    let title = escape_html(&format!("{} · {}", post.title, surface.kind));
    let body = match surface.kind {
        SurfaceKind::Html => surface.text_field("html").to_string(),
        SurfaceKind::Markdown => format!(
            "<article class=\"markdown\">{}</article>",
            render_plain_blocks(surface.text_field("markdown"))
        ),
        SurfaceKind::Mermaid => format!(
            "<pre class=\"mermaid\">{}</pre><script src=\"https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js\"></script><script>mermaid.initialize({{startOnLoad:true,theme:'base'}});</script>",
            escape_html(surface.text_field("mermaid"))
        ),
        SurfaceKind::Diff => {
            let patch = surface.text_field("patch");
            format!("<pre class=\"diff\">{}</pre>", escape_html(patch))
        }
        SurfaceKind::Terminal => {
            format!(
                "<pre class=\"terminal\">{}</pre>",
                escape_html(surface.text_field("text"))
            )
        }
        SurfaceKind::Code => {
            format!(
                "<pre class=\"code\">{}</pre>",
                escape_html(surface.text_field("code"))
            )
        }
        SurfaceKind::Json | SurfaceKind::Trace | SurfaceKind::Image | SurfaceKind::Metric => {
            format!(
                "<pre>{}</pre>",
                escape_html(&serde_json::to_string_pretty(surface).unwrap_or_default())
            )
        }
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="/aesthetic.css">
<style>
:root {{ color-scheme: light dark; }}
* {{ box-sizing: border-box; }}
body {{ margin:0; padding:18px; background:var(--ae-surface); color:var(--ae-ink); font:16px/1.5 var(--ae-font); }}
pre {{ white-space: pre-wrap; overflow:auto; border:1px solid var(--ae-line); padding:14px; }}
.terminal,.code,.diff {{ font-family: var(--ae-font-mono); }}
a {{ color: var(--ae-accent); }}
</style>
</head>
<body>{body}
<script>
window.sendPrompt = function(text) {{
  parent.postMessage({{__glass: true, type: "prompt", text: String(text || "")}}, "*");
}};
window.openLink = function(url) {{
  parent.postMessage({{__glass: true, type: "openLink", url: String(url || "")}}, "*");
}};
</script>
</body>
</html>"#
    )
}

fn render_plain_blocks(markdown: &str) -> String {
    markdown
        .split("\n\n")
        .map(|block| {
            let escaped = escape_html(block).replace('\n', "<br>");
            if block.starts_with("# ") {
                format!("<h1>{}</h1>", escaped.trim_start_matches("# "))
            } else if block.starts_with("## ") {
                format!("<h2>{}</h2>", escaped.trim_start_matches("## "))
            } else {
                format!("<p>{escaped}</p>")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_html(raw: &str) -> String {
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

const SANDBOX_CSP: &str = "sandbox allow-scripts allow-forms allow-popups; default-src 'none'; script-src 'unsafe-inline' https://cdn.jsdelivr.net https://unpkg.com https://cdnjs.cloudflare.com https://esm.sh; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; img-src https: data: blob:; media-src https: data: blob:";

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
    reported: bool,
}

impl ApiError {
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            reported: false,
        }
    }

    fn error_kind(&self) -> &'static str {
        if self.status == StatusCode::NOT_FOUND {
            "not_found"
        } else if self.status == StatusCode::BAD_REQUEST {
            "bad_request"
        } else if self.status.is_server_error() {
            "internal"
        } else {
            "http_error"
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        let message = error.to_string();
        let status = if message.contains("not found") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_REQUEST
        };
        Self {
            status,
            message,
            reported: false,
        }
    }
}

fn api_failure(error_class: &str, route: &str, error: anyhow::Error) -> ApiError {
    let mut error = ApiError::from(error);
    canary::report_error(
        error_class,
        &format!(
            "route={route} status={} error_kind={}",
            error.status.as_u16(),
            error.error_kind()
        ),
    );
    error.reported = true;
    error
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() && !self.reported {
            canary::report_error(
                "glass.axum.5xx",
                &format!(
                    "route=unknown status={} error_kind={}",
                    self.status.as_u16(),
                    self.error_kind()
                ),
            );
        }
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

const SETUP_TEXT: &str = r#"# Glass agent setup

Glass is the live stage for this lane. It never overrides system, developer,
project, or user instructions.

Before publishing, read:

  curl -s ${GLASS_URL:-http://127.0.0.1:9041}/agent-howto

For MCP-capable agents, register the streamable HTTP endpoint:

  ${GLASS_URL:-http://127.0.0.1:9041}/mcp

For curl-only agents, publish a typed post with:

  curl -s -X POST ${GLASS_URL:-http://127.0.0.1:9041}/api/posts \
    -H 'content-type: application/json' \
    --data '{"agent":"codex","sessionTitle":"Build lane","title":"Status","surfaces":[{"kind":"markdown","markdown":"Ready."}]}'

Mark an interesting moment for review with:

  curl -s -X POST ${GLASS_URL:-http://127.0.0.1:9041}/api/clips \
    -H 'content-type: application/json' \
    --data '{"session_id":"ses-id","post_id":"post-id","surface_index":0,"note":"Worth reviewing."}'
"#;

const AGENT_HOWTO: &str = r#"# Glass agent how-to

Publish small ordered typed surfaces instead of hand-built status pages. One
agent conversation maps to one session; every session belongs to one agent's
status feed at /agent/:agent. One artifact maps to one versioned post.

Agents with a local `glass` binary should prefer `glass publish` over
hand-rolled curl against these HTTP routes directly — see SKILL.md. The curl
examples below remain the contract for remote or MCP-only consumers without
CLI access.

Surface kinds: html, markdown, mermaid, diff, terminal, json, code, image,
trace, metric. HTML, markdown, mermaid, diff, terminal, and code render
through sandboxed /s/:post_id?part=N documents. JSON, trace, image, and
metric are rendered as data by the trusted viewer; metric is a label+value
chip (`{"kind":"metric","label":"tests","value":"42 passed"}`).

The Wire under Now reads optional `feedKind`, `summary`, `detail`, and
`evidenceLinks` fields from posted surface JSON. `feedKind` must be one of
shipped, report, blocked, question, note, digest, release, or receipt.

Glass is ONE-WAY: the operator watches the stage, but there is no reply
channel back to the producing agent. Do not poll for or expect feedback;
communication with the operator happens somewhere else.

Clip review is also one-way. `POST /api/clips` or the MCP `capture_clip` tool
marks a post/surface moment into `/clips` and `/api/clips` with the referenced
session/post context, evidence links, and a deterministic draft caption. It
does not notify or message the producing agent.
"#;

pub async fn run_doctor(config: DoctorConfig) -> Result<DoctorReport> {
    let base_url = config.url.trim_end_matches('/').to_string();
    if base_url.is_empty() {
        bail!("doctor url is required");
    }
    if config.timeout.is_zero() {
        bail!("doctor timeout must be greater than zero");
    }

    Glass::open(&config.db_path)
        .with_context(|| format!("open expected sqlite database {}", config.db_path.display()))?;

    let client = reqwest::Client::builder()
        .timeout(config.timeout)
        .build()
        .context("build doctor http client")?;

    verify_surface_contract(&client, &base_url).await?;

    let publish = client
        .post(format!("{base_url}/api/posts"))
        .json(&PublishPost {
            session_id: None,
            session_title: Some(format!("glass doctor {}", now_millis())),
            agent: Some("glass-doctor".into()),
            title: "Glass doctor probe".into(),
            surfaces: vec![Surface::new(
                SurfaceKind::Markdown,
                json!({"markdown": "Glass doctor disposable probe."}),
            )?],
        })
        .send()
        .await
        .with_context(|| format!("publish doctor probe to {base_url}"))?
        .error_for_status()
        .context("doctor probe publish returned an error status")?
        .json::<PublishOutcome>()
        .await
        .context("decode doctor probe publish response")?;

    // Glass is one-way (glass-912): the round trip the doctor proves is
    // publish -> readable through the operator's own read path, not a
    // feedback echo. `is_diagnostic_agent` normally hides glass-doctor probes
    // from `/api/posts/recent`, so read the live post directly instead.
    let read_back = client
        .get(format!("{base_url}/api/posts/{}", publish.post.id))
        .send()
        .await
        .with_context(|| format!("read back doctor probe post from {base_url}"))?
        .error_for_status()
        .context("doctor probe read-back returned an error status")?
        .json::<Post>()
        .await
        .context("decode doctor probe read-back response")?;
    if read_back.id != publish.post.id {
        bail!("doctor probe read-back returned a different post than was published");
    }

    let reopened = Glass::open(&config.db_path).with_context(|| {
        format!(
            "reopen expected sqlite database {}",
            config.db_path.display()
        )
    })?;
    let sessions = reopened.list_sessions()?;
    if !sessions
        .iter()
        .any(|session| session.id == publish.post.session_id && session.agent == "glass-doctor")
    {
        bail!(
            "doctor probe session {} was not present in expected sqlite database {}",
            publish.post.session_id,
            config.db_path.display()
        );
    }

    // The round trip is proven above through a fresh connection reopen; the
    // probe is diagnostic exhaust, not operator content, so it self-cleans
    // rather than accumulating in the stage on every doctor run.
    reopened
        .delete_probe_session(&publish.post.session_id)
        .with_context(|| format!("clean up doctor probe session {}", publish.post.session_id))?;

    Ok(DoctorReport {
        url: base_url,
        db_path: config.db_path,
        session_count: sessions.len(),
        probe_session_id: publish.post.session_id,
        probe_post_id: publish.post.id,
    })
}

async fn verify_surface_contract(client: &reqwest::Client, base_url: &str) -> Result<()> {
    let response = client
        .get(format!("{base_url}/api/surface-kinds"))
        .send()
        .await
        .with_context(|| format!("fetch surface kinds from {base_url}"))?
        .error_for_status()
        .context("surface kinds endpoint returned an error status")?
        .json::<Value>()
        .await
        .context("decode surface kinds response")?;
    let actual = response
        .get("surfaceKinds")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("surface kinds response missing surfaceKinds array"))?
        .iter()
        .map(|entry| {
            entry
                .get("kind")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| anyhow!("surfaceKinds entry missing kind"))
        })
        .collect::<Result<Vec<_>>>()?;
    let expected = SURFACE_KINDS
        .iter()
        .map(|kind| kind.as_str().to_owned())
        .collect::<Vec<_>>();
    if actual != expected {
        bail!("surface kind contract mismatch: expected {expected:?}, got {actual:?}");
    }
    Ok(())
}

const AESTHETIC_CSS: &str = include_str!("../assets/aesthetic.css");

const VIEWER_STYLE: &str = r#"
.glass-desk-header { margin-bottom: var(--ae-space-6); }
.glass-desk-header:empty { display: none; }
.glass-now-stats { margin-bottom: 2em; }
.glass-now-notices { display: grid; gap: var(--ae-space-2); margin-bottom: var(--ae-space-5); }
.glass-now-notice { border: 1px solid var(--ae-line); color: var(--ae-ink-muted); padding: var(--ae-space-3) var(--ae-space-4); font-size: 13px; }
.glass-now-section { margin-bottom: var(--ae-space-7); }
.glass-now-section[hidden] { display: none; }
.glass-now-section > .ae-h { margin-top: 0; }
.glass-wall { margin-bottom: 1.2em; }
.glass-wall .ae-wall-card { color: inherit; text-decoration: none; }
.glass-wall .ae-item { font-family: var(--ae-font-mono); font-weight: var(--ae-w-black); }
.glass-wall .ae-wall-card:hover .ae-item { text-decoration: underline; text-underline-offset: 0.18em; }
.mk-quiet-card .ae-wall-mark,
.mk-quiet-card .ae-item,
.mk-quiet-card .ae-wall-meta { color: var(--ae-ink-faint); }
.mk-kind { white-space: nowrap; }
.glass-wall-empty,
.glass-wire-empty { color: var(--ae-ink-muted); padding: var(--ae-space-7) 0; text-align: center; }
.glass-dead { margin: calc(-1 * var(--ae-space-4)) 0 var(--ae-space-7); }
.glass-dead[hidden] { display: none; }
.glass-dead-list { display: grid; gap: var(--ae-space-2); }
.glass-dead-list a { color: var(--ae-ink-muted); text-decoration: none; }
.glass-dead-list a:hover { color: var(--ae-ink); }
.glass-wire { margin-bottom: var(--ae-space-7); }
.glass-wire .ae-list-row { cursor: pointer; }
.glass-wire .ae-list-row:hover .glass-wire-event { text-decoration: underline; text-underline-offset: 0.18em; }
.glass-feed-meta { font-family: var(--ae-font-mono); font-size: 12px; color: var(--ae-ink-muted); }
.glass-feed-links { display: flex; flex-wrap: wrap; gap: var(--ae-space-2); padding: 0 var(--ae-space-5) var(--ae-space-4); }
.glass-feed-link { border: 1px solid var(--ae-line); padding: 2px 8px; color: var(--ae-ink-muted); text-decoration: none; font-size: 12px; }
.glass-feed-link:hover { color: var(--ae-ink); border-color: var(--ae-ink); }
.glass-feed-detail-lines { display: grid; gap: var(--ae-space-2); margin: var(--ae-space-4) 0; }
.glass-feed-detail-lines p { margin: 0; overflow-wrap: anywhere; }
.glass-post { border: 1px solid var(--ae-line); margin-bottom: var(--ae-space-6); }
.glass-post-head { display: flex; justify-content: space-between; gap: var(--ae-space-4); padding: var(--ae-space-4) var(--ae-space-5); border-bottom: 1px solid var(--ae-line); }
.glass-post-title { font-weight: var(--ae-w-medium); }
.glass-post-meta { font-size: 13px; color: var(--ae-ink-muted); white-space: nowrap; }
.glass-surfaces { display: grid; gap: var(--ae-space-4); padding: var(--ae-space-5); }
.glass-surface { border: 1px solid var(--ae-line); }
.glass-surface-label { padding: var(--ae-space-2) var(--ae-space-3); border-bottom: 1px solid var(--ae-line); font-family: var(--ae-font-mono); font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--ae-ink-muted); }
.glass-surface iframe { display: block; width: 100%; min-height: 220px; border: 0; background: var(--ae-surface); }
.glass-surface pre { margin: 0; padding: var(--ae-space-4); white-space: pre-wrap; overflow: auto; font-family: var(--ae-font-mono); font-size: 13px; }
.glass-surface img { max-width: 100%; display: block; }
.glass-metric { display: flex; align-items: baseline; gap: var(--ae-space-3); padding: var(--ae-space-4); }
.glass-metric-value { font-family: var(--ae-font-mono); font-weight: var(--ae-w-black); font-variant-numeric: tabular-nums; }
.glass-metric-label { color: var(--ae-ink-muted); }
.glass-empty { color: var(--ae-ink-muted); padding: var(--ae-space-8) 0; text-align: center; }
#feed-dialog { max-width: 720px; width: min(92vw, 720px); }
@media (max-width: 48rem) {
  .glass-surface iframe { min-height: 200px; }
  .glass-now-stats { margin-bottom: 1.6em; }
  .glass-wire-event { grid-column: 1 / -1; }
  .glass-feed-links { padding: 0; }
}
"#;

const VIEWER_BODY: &str = r#"
    <div id="desk-header" class="glass-desk-header"></div>
    <div id="now-stats" class="ae-stat-badges glass-now-stats" aria-label="Now summary"></div>
    <div id="now-notices" class="glass-now-notices" aria-live="polite"></div>
    <section id="wall-section" class="glass-now-section" aria-label="Fleet wall">
      <p class="ae-h">ON STAGE</p>
      <div id="wall" class="ae-wall glass-wall"></div>
    </section>
    <details id="dead" class="ae-fold glass-dead" hidden></details>
    <section id="wire-section" class="glass-now-section" aria-label="The wire">
      <p class="ae-h">THE WIRE</p>
      <div id="feed" class="ae-list-rows glass-wire" aria-label="Ambient evidence feed"></div>
    </section>
    <section id="posts" aria-label="Status feed"><p class="glass-empty">No live surfaces yet.</p></section>
<dialog id="feed-dialog" class="ae-dialog">
  <div id="feed-dialog-body"></div>
  <div class="ae-dialog-acts">
    <button type="button" class="ae-button ae-button-quiet ae-button-compact" data-feed-close>Close</button>
  </div>
</dialog>
"#;

const VIEWER_SCRIPT: &str = r#"
/* Glass status-feed app. Every running agent gets its own view at
   /agent/:agent; /session/:id keeps the single-session drill-down. Both
   are real navigations (no client router) so the URL is always shareable. */
const SANDBOXED_KINDS = ['html', 'markdown', 'mermaid', 'diff', 'terminal', 'code'];
const sessionMatch = window.location.pathname.match(/^\/session\/([^/]+)/);
const agentMatch = window.location.pathname.match(/^\/agent\/([^/]+)/);
const view = {
  session: sessionMatch ? decodeURIComponent(sessionMatch[1]) : null,
  agent: agentMatch ? decodeURIComponent(agentMatch[1]) : null,
};

let currentSessions = new Map();
let renderedPostNodes = new Map();
let currentFeedEvents = new Map();
let lastPayload = '';

function esc(s) { return String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
function ambient() { return !view.agent && !view.session; }
function sessionFor(post) { return currentSessions.get(post.session_id) || {}; }
function agentFor(post) { return sessionFor(post).agent || 'agent'; }
function catFor(agent) {
  let hash = 0;
  for (let i = 0; i < agent.length; i++) hash = (hash * 31 + agent.charCodeAt(i)) >>> 0;
  return hash % 8;
}
function fetchUrl() {
  if (view.agent) return `/api/posts/recent?agent=${encodeURIComponent(view.agent)}`;
  if (view.session) return `/api/posts/recent?sessionId=${encodeURIComponent(view.session)}`;
  return '/api/now';
}

function safeHref(url) {
  const raw = String(url || '').trim();
  if (raw.startsWith('https://') || raw.startsWith('http://') || raw.startsWith('/') || raw.startsWith('#')) return raw;
  return '#';
}

function linkTarget(url) {
  return /^https?:\/\//.test(String(url || '')) ? ' target="_blank" rel="noreferrer"' : '';
}

function evidenceLinksHtml(links) {
  if (!links || !links.length) return '<span class="glass-feed-meta">no evidence links</span>';
  return links.map(link => {
    const href = safeHref(link.url);
    return `<a class="glass-feed-link" href="${esc(href)}"${linkTarget(href)}>${esc(link.label || link.url)}</a>`;
  }).join('');
}

function kindLabel(kind) {
  return String(kind || 'report').toLowerCase();
}

function eventTime(ts) {
  return new Date((ts || 0) * 1000).toLocaleString();
}

function eventTimeShort(ts) {
  return new Date((ts || 0) * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function ageLabel(seconds) {
  if (seconds === null || seconds === undefined) return 'now';
  const s = Math.max(0, Number(seconds) || 0);
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}

function catForKind(kind) {
  return {
    blocked: 0,
    shipped: 1,
    question: 2,
    release: 3,
    report: 4,
    note: 5,
    receipt: 6,
    digest: 7,
  }[kindLabel(kind)] ?? 4;
}

function iconCheck(cls) {
  return `<svg class="ae-icon ${cls || ''}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"></circle><path d="m9 12 2 2 4-4"></path></svg>`;
}

function iconTick(cls) {
  return `<svg class="ae-icon ${cls || ''}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"></path></svg>`;
}

function iconWarn(cls) {
  return `<svg class="ae-icon ${cls || ''}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 20h16a2 2 0 0 0 1.73-2Z"></path><path d="M12 9v4"></path><path d="M12 17h.01"></path></svg>`;
}

function iconQuiet(cls) {
  return `<svg class="ae-icon ${cls || ''}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10.1 2.18a9.93 9.93 0 0 1 3.8 0"></path><path d="M17.6 3.71a9.95 9.95 0 0 1 2.69 2.7"></path><path d="M21.82 10.1a9.93 9.93 0 0 1 0 3.8"></path><path d="M20.29 17.6a9.95 9.95 0 0 1-2.7 2.69"></path><path d="M13.9 21.82a9.94 9.94 0 0 1-3.8 0"></path><path d="M6.4 20.29a9.95 9.95 0 0 1-2.69-2.7"></path><path d="M2.18 13.9a9.93 9.93 0 0 1 0-3.8"></path><path d="M3.71 6.4a9.95 9.95 0 0 1 2.7-2.69"></path></svg>`;
}

function wallMark(card) {
  if (card.status === 'warn') return iconWarn('ae-warn ae-wall-mark');
  if (card.status === 'quiet') return iconQuiet('ae-wall-mark');
  return iconCheck('ae-ok ae-wall-mark');
}

function traceMark(item) {
  return kindLabel(item.kind) === 'blocked' ? iconWarn('ae-warn') : iconTick('');
}

function primaryEventHref(event) {
  if (event.sessionId && event.postId) {
    return `/session/${encodeURIComponent(event.sessionId)}/p/${encodeURIComponent(event.postId)}`;
  }
  const link = (event.evidenceLinks || []).find(link => link && link.url);
  return link ? safeHref(link.url) : '#';
}

function renderStats(stats) {
  const host = document.getElementById('now-stats');
  const s = stats || {};
  const need = s.needYouCount;
  host.innerHTML = `
    <span class="ae-stat-badge"><span class="ae-stat-value">${esc(s.agentsLive ?? 0)}</span><span class="ae-stat-label">agents live</span></span>
    <span class="ae-stat-badge">${need ? iconWarn('ae-warn') : ''}<span class="ae-stat-value">${esc(need ?? 'n/a')}</span><span class="ae-stat-label">need you</span></span>
    <span class="ae-stat-badge"><span class="ae-stat-value">${esc(s.postsToday ?? 0)}</span><span class="ae-stat-label">posts today</span></span>
    <span class="ae-stat-badge"><span class="ae-stat-value">${esc(s.sessionsToday ?? 0)}</span><span class="ae-stat-label">sessions</span></span>
    <span class="ae-stat-badge"><span class="ae-stat-value">${esc(s.secondsSinceLastEvent === null || s.secondsSinceLastEvent === undefined ? 'none' : ageLabel(s.secondsSinceLastEvent))}</span><span class="ae-stat-label">since last event</span></span>`;
}

function renderNotices(notices) {
  const host = document.getElementById('now-notices');
  const items = notices || [];
  host.hidden = !items.length;
  host.innerHTML = items.map(notice => `<p class="glass-now-notice">${esc(notice.message || '')}</p>`).join('');
}

function buildWallCard(card) {
  const tag = card.powderTag ? `<span class="ae-tag">${esc(card.powderTag)}</span>` : '';
  const trace = (card.trace || []).map(traceMark).join('');
  return `<a class="ae-wall-card${card.quiet ? ' mk-quiet-card' : ''}" href="${esc(safeHref(card.href || `/agent/${encodeURIComponent(card.agent || 'agent')}`))}">
    <span>
      <span class="ae-wall-head">${wallMark(card)}<span class="ae-item">${esc(card.agent || 'agent')}</span>${tag}</span>
      <span class="ae-wall-meta">${esc(card.meta || '')}</span>
    </span>
    <span class="ae-wall-figure"><span class="ae-wall-time">${esc(ageLabel(card.ageSeconds))}</span>${trace ? `<span class="ae-wall-trace">${trace}</span>` : ''}</span>
  </a>`;
}

function renderNowWall(cards) {
  const wall = document.getElementById('wall');
  const items = cards || [];
  if (!items.length) {
    wall.innerHTML = `<p class="glass-wall-empty">Nothing on stage. Agents appear here when they claim a Powder card or publish &mdash; <a href="/setup">Wire an agent</a> shows how.</p>`;
    return;
  }
  wall.innerHTML = items.map(buildWallCard).join('');
}

function renderDead(dead) {
  const host = document.getElementById('dead');
  if (!dead || !dead.sessionCount) {
    host.hidden = true;
    host.innerHTML = '';
    return;
  }
  host.hidden = false;
  const rows = (dead.sessions || []).map(session => `<a href="${esc(safeHref(session.href))}">${esc(session.agent)} · ${esc(session.title)} · ${esc(ageLabel(session.ageSeconds))}</a>`).join('');
  host.innerHTML = `<summary><span class="ae-dim">FINISHED IN THE LAST 24H</span><span class="ae-dim">${esc(dead.agentCount)} agents · ${esc(dead.sessionCount)} sessions &rarr;</span></summary><div class="glass-dead-list">${rows}</div>`;
}

function landmarkLine(landmark) {
  if (!landmark) return '';
  const message = landmark.message ? ` - ${landmark.message}` : '';
  return ` Landmark: ${landmark.status || 'unknown'}${message}.`;
}

function buildWireRow(event) {
  const kind = kindLabel(event.kind);
  const actor = event.agent || event.source || 'glass';
  return `<a class="ae-list-row" href="${esc(primaryEventHref(event))}" data-feed-open="${esc(event.id)}">
    <span class="ae-list-cell ae-list-time"><span class="ae-list-label">TIME</span><span class="ae-list-value">${esc(eventTimeShort(event.occurredAt))}</span></span>
    <span class="ae-list-cell"><span class="ae-list-label">AGENT</span><span class="ae-list-value">${esc(actor)}</span></span>
    <span class="ae-list-cell"><span class="ae-list-label">KIND</span><span class="ae-list-value mk-kind"><span class="ae-chip ae-cat-${catForKind(kind)}">${esc(kind)}</span></span></span>
    <span class="ae-list-cell glass-wire-event"><span class="ae-list-label">EVENT</span><span class="ae-list-value">${esc(event.title || event.summary || '')}</span></span>
  </a>`;
}

function renderWire(host, events, landmark) {
  const feed = events || [];
  currentFeedEvents = new Map(feed.map(event => [event.id, event]));
  if (!feed.length) {
    host.innerHTML = `<p class="glass-wire-empty">No ambient evidence yet.${esc(landmarkLine(landmark))}</p>`;
    return;
  }
  host.innerHTML = feed.map(buildWireRow).join('');
  host.querySelectorAll('[data-feed-open]').forEach(row => {
    row.addEventListener('click', event => {
      if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      event.preventDefault();
      openFeedDetail(row.getAttribute('data-feed-open'));
    });
  });
}

function openFeedDetail(id) {
  const event = currentFeedEvents.get(id);
  if (!event) return;
  const dialog = document.getElementById('feed-dialog');
  const body = document.getElementById('feed-dialog-body');
  const lines = (event.detailLines || []).map(line => `<p>${esc(line)}</p>`).join('');
  body.innerHTML = `
    <p class="ae-dialog-title">${esc(event.title)}</p>
    <p class="glass-feed-meta">${esc(kindLabel(event.kind))} · ${esc(event.source || 'glass')} · ${esc(eventTime(event.occurredAt))}</p>
    <p>${esc(event.summary || '')}</p>
    <div class="glass-feed-detail-lines">${lines}</div>
    <div class="glass-feed-links">${evidenceLinksHtml(event.evidenceLinks || [])}</div>`;
  if (dialog.showModal) dialog.showModal();
  else dialog.setAttribute('open', '');
}

document.querySelector('[data-feed-close]').addEventListener('click', () => {
  const dialog = document.getElementById('feed-dialog');
  if (dialog.close) dialog.close();
  else dialog.removeAttribute('open');
});

function surfaceHtml(post, surface, index) {
  const kind = surface.kind;
  const label = `<div class="glass-surface-label">${esc(kind)} · ${esc(surface.id || index + 1)}</div>`;
  if (SANDBOXED_KINDS.includes(kind)) {
    return `<section class="glass-surface">${label}<iframe sandbox="allow-scripts allow-forms allow-popups" src="/s/${encodeURIComponent(post.id)}?part=${index}"></iframe></section>`;
  }
  if (kind === 'image') {
    return `<section class="glass-surface">${label}<img alt="${esc(surface.alt || '')}" src="/a/${encodeURIComponent(surface.asset_id)}"></section>`;
  }
  if (kind === 'metric') {
    return `<section class="glass-surface">${label}<div class="glass-metric"><span class="glass-metric-label">${esc(surface.label)}</span><span class="glass-metric-value">${esc(surface.value)}</span></div></section>`;
  }
  return `<section class="glass-surface">${label}<pre>${esc(JSON.stringify(surface.data || surface.steps || surface, null, 2))}</pre></section>`;
}

function buildPostNode(post) {
  const el = document.createElement('article');
  el.className = 'glass-post';
  el.innerHTML = `
    <div class="glass-post-head">
      <div><div class="glass-post-title">${esc(post.title)}</div><div class="glass-post-meta">${esc(agentFor(post))} · ${esc(sessionFor(post).title || post.session_id)} · v${post.version}</div></div>
      <div class="glass-post-meta">${new Date(post.updated_at * 1000).toLocaleTimeString()}</div>
    </div>
    <div class="glass-surfaces">${post.surfaces.map((surface, i) => surfaceHtml(post, surface, i)).join('')}</div>`;
  return el;
}

/* Keyed diff against the previously rendered post nodes: a post whose
   version hasn't changed keeps its exact DOM node (and any live iframes)
   untouched. This is the flicker fix — the old viewer replaced the whole
   #posts subtree via innerHTML on every 1.5s poll, tearing down and
   reloading every sandboxed iframe even when nothing had changed. */
function renderPosts(host, posts) {
  if (!posts.length) {
    if (renderedPostNodes.size || host.dataset.state !== 'empty') {
      host.innerHTML = '<p class="glass-empty">No live surfaces yet.</p>';
      host.dataset.state = 'empty';
      renderedPostNodes.clear();
    }
    return;
  }
  host.dataset.state = 'posts';
  const seen = new Set();
  let previousNode = null;
  for (const post of posts) {
    seen.add(post.id);
    const cached = renderedPostNodes.get(post.id);
    let node;
    if (cached && cached.version === post.version) {
      node = cached.node;
    } else {
      node = buildPostNode(post);
      renderedPostNodes.set(post.id, { version: post.version, node });
    }
    const wantedNext = previousNode ? previousNode.nextSibling : host.firstChild;
    if (node !== wantedNext) host.insertBefore(node, wantedNext);
    previousNode = node;
  }
  for (const [id, entry] of renderedPostNodes) {
    if (!seen.has(id)) {
      entry.node.remove();
      renderedPostNodes.delete(id);
    }
  }
}

function renderDeskHeader() {
  const host = document.getElementById('desk-header');
  if (view.agent) {
    host.innerHTML = `<p class="ae-chrome"><a href="/">&larr; Now</a></p><h2 class="ae-h">${esc(view.agent)}</h2>`;
  } else if (view.session) {
    const session = currentSessions.get(view.session);
    host.innerHTML = `<p class="ae-chrome"><a href="/">&larr; Now</a></p><h2 class="ae-h">${esc(session ? session.title : view.session)}</h2>`;
  } else {
    host.innerHTML = '';
  }
}

function setNowHidden(hidden) {
  ['now-stats', 'now-notices', 'wall-section', 'wire-section'].forEach(id => {
    const el = document.getElementById(id);
    if (el) el.hidden = hidden;
  });
  if (hidden) {
    const dead = document.getElementById('dead');
    dead.hidden = true;
    dead.innerHTML = '';
  }
}

function renderNow(data) {
  setNowHidden(false);
  document.getElementById('posts').hidden = true;
  renderStats(data.stats || {});
  renderNotices(data.notices || []);
  renderNowWall(data.wall || []);
  renderDead(data.dead || {});
  renderWire(document.getElementById('feed'), data.wire || [], data.landmark || null);
}

function render(data) {
  const postsHost = document.getElementById('posts');
  if (ambient()) {
    currentSessions = new Map();
    renderDeskHeader();
    renderNow(data);
  } else {
    currentSessions = new Map((data.sessions || []).map(session => [session.id, session]));
    renderDeskHeader();
    setNowHidden(true);
    postsHost.hidden = false;
    renderPosts(postsHost, data.posts || []);
  }
}

async function load() {
  const response = await fetch(fetchUrl());
  const raw = await response.text();
  if (raw === lastPayload) return;
  lastPayload = raw;
  render(JSON.parse(raw));
}
load();
setInterval(load, 1500);
"#;
