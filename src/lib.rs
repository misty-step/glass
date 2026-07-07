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
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

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
        .route("/setup", get(setup))
        .route("/agent-howto", get(agent_howto))
        .route("/api/surface-kinds", get(surface_kinds))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/posts", post(publish_post))
        .route("/api/posts/recent", get(recent_posts))
        .route("/api/posts/{id}", get(get_post).put(update_post))
        .route("/api/assets", post(upload_asset))
        .route("/a/{id}", get(serve_asset))
        .route("/s/{post_id}", get(render_sandbox))
        .route("/mcp", post(mcp))
        .with_state(glass)
}

async fn viewer() -> Html<String> {
    Html(VIEWER_HTML.replace("{{SANCTUM_URL}}", &escape_html(&sanctum_url())))
}

/// The cross-repo "back to Sanctum" link (glass-915): config-driven via
/// `GLASS_SANCTUM_URL` rather than a hardcoded personal tailnet hostname, so
/// forks of this public repo don't inherit a link into the origin
/// deployment's infrastructure. Deployments that sit behind a Sanctum portal
/// set the env var to the portal root; unset, the link is inert (`/`).
fn sanctum_url() -> String {
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
    Ok(Json(glass.create_session(input)?))
}

async fn list_sessions(State(glass): State<Glass>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "sessions": glass.list_sessions()? })))
}

async fn publish_post(
    State(glass): State<Glass>,
    Json(input): Json<PublishPost>,
) -> Result<Json<PublishOutcome>, ApiError> {
    Ok(Json(glass.publish_post(input)?))
}

async fn update_post(
    State(glass): State<Glass>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<PublishPost>,
) -> Result<Json<PublishOutcome>, ApiError> {
    Ok(Json(glass.update_post(&id, input)?))
}

async fn get_post(
    State(glass): State<Glass>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Post>, ApiError> {
    Ok(Json(glass.get_post(&id)?))
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

async fn recent_posts(
    State(glass): State<Glass>,
    Query(query): Query<RecentQuery>,
) -> Result<Json<Value>, ApiError> {
    let fetch_limit = if query.agent.is_some() || query.session_id.is_some() {
        100
    } else {
        query.limit.unwrap_or(30)
    };
    let raw_posts = glass.list_recent_posts(fetch_limit)?;
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
    let asset = glass.store_asset(content_type, filename, &body)?;
    Ok(Json(
        json!({ "asset": asset, "url": format!("/a/{}", asset.id) }),
    ))
}

async fn serve_asset(
    State(glass): State<Glass>,
    AxumPath(id): AxumPath<String>,
) -> Result<Response, ApiError> {
    let (asset, data) = glass.load_asset(&id)?;
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
    let post = glass.get_post(&post_id)?;
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
        Err(error) => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32602, "message": error.to_string() }
        })),
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
                _ => Err(anyhow!("unknown tool: {name}")),
            }
        }
        _ => Err(anyhow!("unsupported JSON-RPC method")),
    }
}

fn mcp_tools() -> Vec<Value> {
    vec![json!({
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
    })]
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
}

impl ApiError {
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
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
        Self { status, message }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
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

Glass is ONE-WAY: the operator watches the stage, but there is no reply
channel back to the producing agent. Do not poll for or expect feedback;
communication with the operator happens somewhere else.
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

const VIEWER_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Glass</title>
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
.glass-wall { margin-bottom: var(--ae-space-7); }
.glass-wall:empty { display: none; }
.glass-dead { margin: 0 0 var(--ae-space-7); }
.glass-dead summary { cursor: pointer; color: var(--ae-ink-muted); font-size: 13px; }
.glass-dead-list { display: grid; gap: var(--ae-space-2); margin-top: var(--ae-space-3); }
.glass-dead-list a { color: var(--ae-ink-muted); text-decoration: none; }
.glass-dead-list a:hover { color: var(--ae-ink); }
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
.glass-desk-header { margin-bottom: var(--ae-space-6); }
#rail-agents { display: grid; gap: var(--ae-space-2); }
@media (max-width: 48rem) {
  .glass-surface iframe { min-height: 200px; }
  /* The kit's own .ae-rail mobile rule expects rail links as its direct
     children so its row flex lays them out edge to edge; #rail-agents
     wraps Glass's dynamic agent list in one div for the renderer, so it
     needs the same row treatment or its children fall back to block
     stacking and the bottom chrome grows tall instead of staying a slim,
     horizontally-scrollable bar. */
  #rail-agents { display: flex; flex-direction: row; gap: var(--ae-space-5); }
}
</style>
</head>
<body>
<div class="ae-shell">
  <aside class="ae-rail">
    <a class="ae-logo ae-logo-compact" href="/">
      <span class="ae-app-mark"><svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M11 6 8 9"></path><path d="m16 7-8 8"></path><rect x="4" y="2" width="16" height="20"></rect></svg></span>
      <span class="ae-name">Glass</span>
    </a>
    <a data-sanctum-home href="{{SANCTUM_URL}}" aria-label="Back to Sanctum" title="Back to Sanctum"><svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"></path><path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"></path></svg> Sanctum</a>
    <p class="ae-h">Agents</p>
    <a href="/" id="rail-all">All agents</a>
    <div id="rail-agents"></div>
    <div class="ae-rail-foot">
      <button class="ae-mode" aria-label="toggle color mode">
        <svg class="ae-icon ae-sun" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="4"></circle><path d="M12 2v2"></path><path d="M12 20v2"></path><path d="m4.93 4.93 1.41 1.41"></path><path d="m17.66 17.66 1.41 1.41"></path><path d="M2 12h2"></path><path d="M20 12h2"></path><path d="m6.34 17.66-1.41 1.41"></path><path d="m19.07 4.93-1.41 1.41"></path></svg>
        <svg class="ae-icon ae-moon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"></path></svg>
      </button>
    </div>
  </aside>
  <main class="ae-desk">
    <div id="desk-header" class="glass-desk-header"></div>
    <section id="wall" class="ae-wall glass-wall" aria-label="Live sessions"></section>
    <details id="dead" class="glass-dead" hidden></details>
    <section id="posts" aria-label="Status feed"><p class="glass-empty">No live surfaces yet.</p></section>
  </main>
</div>
<script>
/* the mode recipe (Misty Step Aesthetic kit), inlined verbatim: toggle
   .dark/.light on the root, pin color-scheme, interruptible transition. */
(() => {
  const root = document.documentElement;
  let activeTransition = null;
  let easingTimer = 0;
  let runId = 0;
  let targetDark = null;
  const isDark = () =>
    root.classList.contains('dark')
      ? true
      : root.classList.contains('light')
        ? false
        : matchMedia('(prefers-color-scheme: dark)').matches;
  const reducedMode = matchMedia('(prefers-reduced-motion: reduce)');
  const clearAnimation = () => {
    if (activeTransition && activeTransition.skipTransition) activeTransition.skipTransition();
    activeTransition = null;
    if (easingTimer) { clearTimeout(easingTimer); easingTimer = 0; }
    root.classList.remove('ae-vt-mode', 'ae-mode-easing');
  };
  const applyMode = (dark) => {
    root.classList.toggle('dark', dark);
    root.classList.toggle('light', !dark);
    root.style.colorScheme = dark ? 'dark' : 'light';
    try { localStorage.setItem('ae-mode', dark ? 'dark' : 'light'); } catch (e) {}
  };
  document.querySelectorAll('.ae-mode').forEach((btn) => {
    btn.addEventListener('click', () => {
      const nextDark = !(targetDark ?? isDark());
      const id = ++runId;
      targetDark = nextDark;
      const flip = () => { if (id !== runId) return; applyMode(nextDark); };
      clearAnimation();
      if (reducedMode.matches) {
        flip();
      } else if (document.startViewTransition) {
        root.classList.add('ae-vt-mode');
        activeTransition = document.startViewTransition(flip);
        easingTimer = setTimeout(() => {
          if (id !== runId) return;
          root.classList.remove('ae-vt-mode');
          easingTimer = 0;
        }, 180);
        activeTransition.finished.finally(() => {
          if (id !== runId) return;
          root.classList.remove('ae-vt-mode');
          activeTransition = null;
          if (easingTimer) { clearTimeout(easingTimer); easingTimer = 0; }
        });
      } else {
        root.classList.add('ae-mode-easing');
        flip();
        easingTimer = setTimeout(() => {
          if (id !== runId) return;
          root.classList.remove('ae-mode-easing');
          easingTimer = 0;
        }, 180);
      }
    });
  });
})();

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
let lastPayload = '';

function esc(s) { return String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
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
  return '/api/posts/recent?limit=40';
}

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
    host.innerHTML = `<p class="ae-chrome"><a href="/">&larr; All agents</a></p><h2 class="ae-h">${esc(view.agent)}</h2>`;
  } else if (view.session) {
    const session = currentSessions.get(view.session);
    host.innerHTML = `<p class="ae-chrome"><a href="/">&larr; All agents</a></p><h2 class="ae-h">${esc(session ? session.title : view.session)}</h2>`;
  } else {
    host.innerHTML = '';
  }
}

function renderRail(agents) {
  const railAll = document.getElementById('rail-all');
  if (!view.agent && !view.session) railAll.setAttribute('aria-current', 'page');
  else railAll.removeAttribute('aria-current');
  const host = document.getElementById('rail-agents');
  host.innerHTML = agents.map(agent => `<a href="/agent/${encodeURIComponent(agent.agent)}" ${view.agent === agent.agent ? 'aria-current="page"' : ''}><span class="ae-chip ae-cat-${catFor(agent.agent)}">${esc(agent.agent)}</span></a>`).join('');
}

/* Dead sessions (no post in LIVE_WINDOW_SECONDS) are demoted out of the
   primary wall into a collapsed archive; they never render as peers of
   live work. */
function renderWall(sessions, posts) {
  const wall = document.getElementById('wall');
  const dead = document.getElementById('dead');
  if (view.agent || view.session) {
    wall.innerHTML = '';
    dead.hidden = true;
    return;
  }
  const latestBySession = new Map();
  for (const post of posts) {
    if (!latestBySession.has(post.session_id)) latestBySession.set(post.session_id, post);
  }
  const withPosts = sessions.filter(session => latestBySession.has(session.id));
  const live = withPosts.filter(session => session.isLive).sort((a, b) => b.last_active_at - a.last_active_at);
  const stale = withPosts.filter(session => !session.isLive).sort((a, b) => b.last_active_at - a.last_active_at);
  wall.innerHTML = live.map(session => {
    const post = latestBySession.get(session.id);
    return `<a class="ae-wall-card" href="/session/${encodeURIComponent(session.id)}">
      <div>
        <div class="ae-wall-head"><span class="ae-chip ae-cat-${catFor(session.agent)}">${esc(session.agent)}</span></div>
        <div class="ae-wall-meta">${esc(session.title)} · ${esc(post.title)}</div>
      </div>
      <div class="ae-wall-figure"><span class="ae-wall-time">${new Date(post.updated_at * 1000).toLocaleTimeString()}</span></div>
    </a>`;
  }).join('');
  if (stale.length) {
    dead.hidden = false;
    dead.innerHTML = `<summary>Dead sessions (${stale.length})</summary><div class="glass-dead-list">${stale.map(session => `<a href="/session/${encodeURIComponent(session.id)}">${esc(session.agent)} · ${esc(session.title)}</a>`).join('')}</div>`;
  } else {
    dead.hidden = true;
    dead.innerHTML = '';
  }
}

function render(data) {
  currentSessions = new Map((data.sessions || []).map(session => [session.id, session]));
  renderDeskHeader();
  renderRail(data.agents || []);
  renderWall(data.sessions || [], data.posts || []);
  renderPosts(document.getElementById('posts'), data.posts || []);
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
</script>
</body>
</html>"#;
