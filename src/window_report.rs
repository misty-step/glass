//! REP-1's data seam (glass-917): Glass becomes the fourth consumer of the
//! fleet-retro/weave synthesis spine instead of building a second one.
//!
//! fleet-retro's scheduled daily/weekly job shelf-publishes `spec.json` at
//! `<shelf-base>/<window>/spec.json`; Glass keeps that fast path unchanged
//! for `daily` and `weekly`. glass-919 adds the consumer-side seam for the
//! future weave on-demand synthesis service: any other window slug is treated
//! as a custom window and, when `GLASS_SYNTHESIS_ENDPOINT` is configured,
//! POSTs `{window, since, until, scope}` there and relays its SSE stream.
//!
//! The synthesis engine still lives in weave/apps/fleet-retro and has no HTTP
//! endpoint today. When the endpoint is unset or unavailable, custom windows
//! emit a clear SSE fallback/error instead of crashing or returning a blank.

use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::{Path as AxumPath, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// fleet-retro currently only produces these two windows on a schedule.
/// Arbitrary/custom windows need an on-demand synthesis service that does
/// not exist yet (glass-919).
pub(crate) const SUPPORTED_WINDOWS: [&str; 2] = ["daily", "weekly"];

const CACHE_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheStatus {
    Hit,
    Miss,
}

#[derive(Debug, Clone)]
enum FetchOutcome {
    Ok(Value),
    Err(String),
}

type Slot = Arc<tokio::sync::Mutex<Option<(Instant, Arc<FetchOutcome>)>>>;

/// Per-(window, scope) single-flight cache. The inner `tokio::sync::Mutex`
/// does double duty: it is both the TTL guard (checked immediately after
/// acquiring the lock) and the coalescing point -- concurrent callers for the
/// same key queue on the same lock, so only the caller that actually finds a
/// stale-or-empty slot ever calls the fetcher.
#[derive(Clone, Default)]
struct WindowReportCache {
    slots: Arc<StdMutex<HashMap<(String, String), Slot>>>,
}

impl WindowReportCache {
    fn slot_for(&self, window: &str, scope: &str) -> Slot {
        let mut slots = self
            .slots
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        slots
            .entry((window.to_string(), scope.to_string()))
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(None)))
            .clone()
    }
}

static CACHE: OnceLock<WindowReportCache> = OnceLock::new();

fn global_cache() -> WindowReportCache {
    CACHE.get_or_init(WindowReportCache::default).clone()
}

/// Coalescing TTL cache lookup: returns the cached outcome if fresh, else
/// runs `fetch` once (whichever caller wins the lock) and caches the result.
async fn get_or_fetch<F, Fut>(
    cache: &WindowReportCache,
    window: &str,
    scope: &str,
    ttl: Duration,
    fetch: F,
) -> (CacheStatus, Arc<FetchOutcome>)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    let slot = cache.slot_for(window, scope);
    let mut guard = slot.lock().await;
    if let Some((fetched_at, outcome)) = guard.as_ref()
        && fetched_at.elapsed() < ttl
    {
        return (CacheStatus::Hit, outcome.clone());
    }
    let outcome = Arc::new(match fetch().await {
        Ok(value) => FetchOutcome::Ok(value),
        Err(err) => FetchOutcome::Err(err),
    });
    *guard = Some((Instant::now(), outcome.clone()));
    (CacheStatus::Miss, outcome)
}

/// Base URL for the bastion artifact shelf's fleet-retro publish path (e.g.
/// `https://<tailnet-host>/artifacts/a/fleet-retro`), config-driven so this
/// public repo never hardcodes a personal tailnet hostname (glass-915).
/// Unset means the deployment hasn't wired fleet-retro publishing.
fn shelf_base_url() -> Option<String> {
    std::env::var("GLASS_FLEET_RETRO_SHELF_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

/// Future weave-side on-demand synthesis endpoint. Unset means custom windows
/// degrade to the nearest shelf window/error path rather than blocking on a
/// service that does not exist yet.
fn synthesis_endpoint_url() -> Option<String> {
    std::env::var("GLASS_SYNTHESIS_ENDPOINT")
        .ok()
        .filter(|value| !value.is_empty())
}

async fn fetch_from_shelf(window: &str) -> Result<Value, String> {
    let base = match shelf_base_url() {
        Some(base) => base,
        None => {
            crate::canary::report_error(
                "glass.window_report.fetch.failed",
                "route=/api/window-report/{window} upstream=fleet-retro-shelf error_kind=missing_shelf_url",
            );
            return Err("GLASS_FLEET_RETRO_SHELF_URL is not configured".to_string());
        }
    };
    let url = format!("{}/{window}/spec.json", base.trim_end_matches('/'));
    let response = reqwest::get(&url).await.map_err(|err| {
        crate::canary::report_error(
            "glass.window_report.fetch.failed",
            "route=/api/window-report/{window} upstream=fleet-retro-shelf error_kind=transport",
        );
        format!("fetch {url}: {err}")
    })?;
    if !response.status().is_success() {
        crate::canary::report_error(
            "glass.window_report.fetch.failed",
            &format!(
                "route=/api/window-report/{{window}} upstream=fleet-retro-shelf upstream_status={} error_kind=upstream_status",
                response.status().as_u16()
            ),
        );
        return Err(format!(
            "fetch {url}: upstream returned {}",
            response.status()
        ));
    }
    response.json::<Value>().await.map_err(|err| {
        crate::canary::report_error(
            "glass.window_report.fetch.failed",
            "route=/api/window-report/{window} upstream=fleet-retro-shelf error_kind=parse",
        );
        format!("parse {url}: {err}")
    })
}

/// Fetch (or return the cached copy of) a window's fleet-retro shelf spec.
/// Shared by `window_report` and glass-913's REP-1 renderer (`crate::rep1`)
/// so both go through the same coalescing TTL cache rather than each
/// maintaining its own.
pub(crate) async fn fetch_window(window: &str, scope: &str) -> (bool, Result<Value, String>) {
    let cache = global_cache();
    let (status, outcome) = get_or_fetch(&cache, window, scope, CACHE_TTL, || {
        fetch_from_shelf(window)
    })
    .await;
    let is_hit = status == CacheStatus::Hit;
    match outcome.as_ref() {
        FetchOutcome::Ok(value) => (is_hit, Ok(value.clone())),
        FetchOutcome::Err(message) => (is_hit, Err(message.clone())),
    }
}

#[derive(Debug, Deserialize)]
pub struct WindowReportQuery {
    scope: Option<String>,
    since: Option<String>,
    until: Option<String>,
}

#[derive(Debug, Serialize)]
struct SynthesisRequest<'a> {
    window: &'a str,
    since: &'a str,
    until: &'a str,
    scope: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
struct UpstreamEvent {
    event: String,
    data: String,
}

impl UpstreamEvent {
    fn into_sse(self) -> Event {
        Event::default()
            .event(safe_event_name(&self.event))
            .data(self.data)
    }
}

#[derive(Default)]
struct SseAccumulator {
    buffer: String,
}

impl SseAccumulator {
    fn push(&mut self, chunk: &str) -> Vec<UpstreamEvent> {
        self.buffer
            .push_str(&chunk.replace("\r\n", "\n").replace('\r', "\n"));
        let mut events = Vec::new();
        while let Some(index) = self.buffer.find("\n\n") {
            let block = self.buffer[..index].to_string();
            self.buffer.drain(..index + 2);
            if let Some(event) = parse_sse_block(&block) {
                events.push(event);
            }
        }
        events
    }

    fn finish(self) -> Option<UpstreamEvent> {
        parse_sse_block(self.buffer.trim_end())
    }
}

fn parse_sse_block(block: &str) -> Option<UpstreamEvent> {
    if block.trim().is_empty() {
        return None;
    }
    let mut event = "message".to_string();
    let mut data = Vec::new();
    for line in block.lines() {
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
    if data.is_empty() {
        return None;
    }
    Some(UpstreamEvent {
        event,
        data: data.join("\n"),
    })
}

fn safe_event_name(name: &str) -> &str {
    if name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        && !name.is_empty()
    {
        name
    } else {
        "partial"
    }
}

fn fallback_shelf_window(window: &str, since: Option<&str>, until: Option<&str>) -> &'static str {
    match window {
        "weekly" | "7d" => return "weekly",
        "daily" | "24h" | "30m" | "1h" => return "daily",
        _ => {}
    }
    let Some((since, until)) = since.zip(until) else {
        return "daily";
    };
    let Ok(since) = DateTime::parse_from_rfc3339(since) else {
        return "daily";
    };
    let Ok(until) = DateTime::parse_from_rfc3339(until) else {
        return "daily";
    };
    if until.signed_duration_since(since).num_seconds() > 2 * 24 * 60 * 60 {
        "weekly"
    } else {
        "daily"
    }
}

async fn fallback_events(
    window: &str,
    scope: &str,
    since: Option<&str>,
    until: Option<&str>,
    reason: impl Into<String>,
) -> Vec<Event> {
    let reason = reason.into();
    let fallback_window = fallback_shelf_window(window, since, until);
    let mut events = vec![
        Event::default().event("fallback").data(
            json!({
                "stage": "fallback",
                "window": window,
                "scope": scope,
                "since": since,
                "until": until,
                "fallback_window": fallback_window,
                "message": reason,
            })
            .to_string(),
        ),
    ];

    let (is_hit, outcome) = fetch_window(fallback_window, scope).await;
    match outcome {
        Ok(spec) => events.push(
            Event::default().event("full").data(
                json!({
                    "stage": "full",
                    "window": window,
                    "scope": scope,
                    "since": since,
                    "until": until,
                    "cache": if is_hit { "hit" } else { "miss" },
                    "fallback": true,
                    "fallback_window": fallback_window,
                    "spec": spec,
                })
                .to_string(),
            ),
        ),
        Err(message) => events.push(
            Event::default().event("error").data(
                json!({
                    "stage": "error",
                    "window": window,
                    "scope": scope,
                    "since": since,
                    "until": until,
                    "fallback_window": fallback_window,
                    "message": format!("{reason}; fallback shelf fetch failed: {message}"),
                })
                .to_string(),
            ),
        ),
    }
    events
}

async fn post_synthesis(
    endpoint: &str,
    request: &SynthesisRequest<'_>,
) -> Result<reqwest::Response, String> {
    reqwest::Client::new()
        .post(endpoint)
        .json(request)
        .send()
        .await
        .map_err(|err| format!("post {endpoint}: {err}"))
}

/// `GET /api/window-report/{window}`.
///
/// `daily` and `weekly` stream the shelf-published fleet-retro spec through
/// Glass's coalescing cache. Any other window slug is a custom window:
/// `?since=...&until=...&scope=...` is POSTed to `GLASS_SYNTHESIS_ENDPOINT`
/// and upstream SSE events are relayed unchanged after Glass's own skeleton
/// event, preserving citation metadata such as `InlineNode::Cite`.
pub async fn window_report(
    AxumPath(window): AxumPath<String>,
    Query(params): Query<WindowReportQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let scope = params.scope.unwrap_or_else(|| "fleet".to_string());
    let since = params.since;
    let until = params.until;
    let cache = global_cache();

    let stream = async_stream::stream! {
        yield Ok::<_, Infallible>(
            Event::default()
                .event("skeleton")
                .data(
                    json!({
                        "stage": "skeleton",
                        "window": window,
                        "scope": scope,
                        "since": since.as_deref(),
                        "until": until.as_deref(),
                    })
                    .to_string(),
                ),
        );

        if !SUPPORTED_WINDOWS.contains(&window.as_str()) {
            let Some(since_value) = since.as_deref() else {
                for event in fallback_events(
                    &window,
                    &scope,
                    None,
                    until.as_deref(),
                    "custom window requests require a since query parameter",
                ).await {
                    yield Ok::<_, Infallible>(event);
                }
                return;
            };
            let Some(until_value) = until.as_deref() else {
                for event in fallback_events(
                    &window,
                    &scope,
                    Some(since_value),
                    None,
                    "custom window requests require an until query parameter",
                ).await {
                    yield Ok::<_, Infallible>(event);
                }
                return;
            };
            let Some(endpoint) = synthesis_endpoint_url() else {
                for event in fallback_events(
                    &window,
                    &scope,
                    Some(since_value),
                    Some(until_value),
                    "GLASS_SYNTHESIS_ENDPOINT is not configured",
                ).await {
                    yield Ok::<_, Infallible>(event);
                }
                return;
            };

            let request = SynthesisRequest {
                window: &window,
                since: since_value,
                until: until_value,
                scope: &scope,
            };
            let mut response = match post_synthesis(&endpoint, &request).await {
                Ok(response) => response,
                Err(message) => {
                    for event in fallback_events(&window, &scope, Some(since_value), Some(until_value), message).await {
                        yield Ok::<_, Infallible>(event);
                    }
                    return;
                }
            };
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                let message = if body.trim().is_empty() {
                    format!("post {endpoint}: upstream returned {status}")
                } else {
                    format!("post {endpoint}: upstream returned {status}: {body}")
                };
                for event in fallback_events(&window, &scope, Some(since_value), Some(until_value), message).await {
                    yield Ok::<_, Infallible>(event);
                }
                return;
            }

            let content_type = response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_string();
            if content_type.contains("text/event-stream") {
                let mut parser = SseAccumulator::default();
                loop {
                    match response.chunk().await {
                        Ok(Some(chunk)) => {
                            let text = String::from_utf8_lossy(&chunk);
                            for upstream in parser.push(&text) {
                                yield Ok::<_, Infallible>(upstream.into_sse());
                            }
                        }
                        Ok(None) => {
                            if let Some(upstream) = parser.finish() {
                                yield Ok::<_, Infallible>(upstream.into_sse());
                            }
                            return;
                        }
                        Err(err) => {
                            yield Ok::<_, Infallible>(
                                Event::default().event("error").data(
                                    json!({
                                        "stage": "error",
                                        "window": window,
                                        "scope": scope,
                                        "since": since_value,
                                        "until": until_value,
                                        "message": format!("read synthesis stream {endpoint}: {err}"),
                                    })
                                    .to_string(),
                                ),
                            );
                            return;
                        }
                    }
                }
            }

            match response.json::<Value>().await {
                Ok(body) => {
                    let data = if body.get("stage").is_some() {
                        body
                    } else {
                        json!({
                            "stage": "full",
                            "window": window,
                            "scope": scope,
                            "since": since_value,
                            "until": until_value,
                            "spec": body,
                        })
                    };
                    yield Ok::<_, Infallible>(Event::default().event("full").data(data.to_string()));
                }
                Err(err) => {
                    for event in fallback_events(
                        &window,
                        &scope,
                        Some(since_value),
                        Some(until_value),
                        format!("parse synthesis response {endpoint}: {err}"),
                    ).await {
                        yield Ok::<_, Infallible>(event);
                    }
                }
            }
            return;
        }

        let (status, outcome) = get_or_fetch(&cache, &window, &scope, CACHE_TTL, || {
            fetch_from_shelf(&window)
        })
        .await;

        match outcome.as_ref() {
            FetchOutcome::Ok(spec) => {
                let cache_label = if status == CacheStatus::Hit { "hit" } else { "miss" };
                yield Ok::<_, Infallible>(
                    Event::default().event("full").data(
                        json!({
                            "stage": "full",
                            "window": window,
                            "scope": scope,
                            "cache": cache_label,
                            "spec": spec,
                        })
                        .to_string(),
                    ),
                );
            }
            FetchOutcome::Err(message) => {
                yield Ok::<_, Infallible>(
                    Event::default().event("error").data(
                        json!({"stage": "error", "window": window, "scope": scope, "message": message})
                            .to_string(),
                    ),
                );
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::Json as AxumJson;
    use axum::http::header;
    use axum::routing::post;
    use http_body_util::BodyExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::task::JoinHandle;

    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: String) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: tests using Glass endpoint env vars take ENV_LOCK so
            // process-global environment mutation stays serialized here.
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }

        fn remove(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: tests using Glass endpoint env vars take ENV_LOCK so
            // process-global environment mutation stays serialized here.
            unsafe {
                std::env::remove_var(key);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: see EnvGuard::set/remove; the lock is held for each
            // test's whole env-using scope, including guard drop.
            unsafe {
                if let Some(previous) = &self.previous {
                    std::env::set_var(self.key, previous);
                } else {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    async fn report_text(window: &str, query: WindowReportQuery) -> String {
        let response = window_report(AxumPath(window.to_string()), Query(query))
            .await
            .expect("window report response")
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect body")
            .to_bytes();
        String::from_utf8(body.to_vec()).expect("utf8 body")
    }

    async fn start_synthesis_mock(
        body: &'static str,
        content_type: &'static str,
    ) -> (String, Arc<StdMutex<Option<Value>>>, JoinHandle<()>) {
        let seen = Arc::new(StdMutex::new(None));
        let app_seen = seen.clone();
        let app = axum::Router::new().route(
            "/synthesize",
            post(move |AxumJson(payload): AxumJson<Value>| {
                let app_seen = app_seen.clone();
                async move {
                    *app_seen.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(payload);
                    ([(header::CONTENT_TYPE, content_type)], Body::from(body))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock synthesis server");
        let addr = listener.local_addr().expect("mock addr");
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve mock");
        });
        (format!("http://{addr}/synthesize"), seen, handle)
    }

    #[tokio::test]
    async fn cache_miss_then_hit_calls_fetch_exactly_once() {
        let cache = WindowReportCache::default();
        let calls = Arc::new(AtomicUsize::new(0));

        let (status, outcome) = get_or_fetch(&cache, "daily", "fleet", Duration::from_secs(60), {
            let calls = calls.clone();
            move || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Ok(json!({"hit": 1})) }
            }
        })
        .await;
        assert_eq!(status, CacheStatus::Miss);
        assert!(matches!(outcome.as_ref(), FetchOutcome::Ok(v) if v == &json!({"hit": 1})));

        let (status, _) = get_or_fetch(&cache, "daily", "fleet", Duration::from_secs(60), {
            let calls = calls.clone();
            move || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Ok(json!({"hit": 2})) }
            }
        })
        .await;
        assert_eq!(status, CacheStatus::Hit);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "cache hit must not re-fetch"
        );
    }

    #[tokio::test]
    async fn expired_ttl_triggers_a_fresh_fetch() {
        let cache = WindowReportCache::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let ttl = Duration::from_millis(5);

        get_or_fetch(&cache, "daily", "fleet", ttl, {
            let calls = calls.clone();
            move || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Ok(json!({})) }
            }
        })
        .await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        let (status, _) = get_or_fetch(&cache, "daily", "fleet", ttl, {
            let calls = calls.clone();
            move || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Ok(json!({})) }
            }
        })
        .await;

        assert_eq!(status, CacheStatus::Miss, "expired entry must be refetched");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn concurrent_requests_for_the_same_bucket_coalesce_to_one_fetch() {
        let cache = Arc::new(WindowReportCache::default());
        let calls = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let cache = cache.clone();
            let calls = calls.clone();
            handles.push(tokio::spawn(async move {
                get_or_fetch(&cache, "weekly", "fleet", Duration::from_secs(60), || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async {
                        tokio::time::sleep(Duration::from_millis(30)).await;
                        Ok(json!({"window": "weekly"}))
                    }
                })
                .await
            }));
        }
        for handle in handles {
            handle.await.expect("task completes");
        }

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "8 concurrent requests for the same (window, scope) bucket must coalesce to one fetch"
        );
    }

    #[tokio::test]
    async fn different_scopes_do_not_share_a_bucket() {
        let cache = WindowReportCache::default();
        let calls = Arc::new(AtomicUsize::new(0));

        for scope in ["fleet", "repo:glass"] {
            get_or_fetch(&cache, "daily", scope, Duration::from_secs(60), {
                let calls = calls.clone();
                move || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { Ok(json!({})) }
                }
            })
            .await;
        }

        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "distinct scopes are distinct buckets"
        );
    }

    #[test]
    fn sse_accumulator_parses_events_and_preserves_json_data() {
        let mut parser = SseAccumulator::default();
        let events = parser.push(
            "event: partial\ndata: {\"stage\":\"partial\"}\n\nevent: full\ndata: {\"spec\":{\"ok\":true}}\n\n",
        );
        assert_eq!(
            events,
            vec![
                UpstreamEvent {
                    event: "partial".to_string(),
                    data: "{\"stage\":\"partial\"}".to_string(),
                },
                UpstreamEvent {
                    event: "full".to_string(),
                    data: "{\"spec\":{\"ok\":true}}".to_string(),
                },
            ]
        );
        assert_eq!(parser.finish(), None);
    }

    #[tokio::test]
    async fn daily_window_keeps_the_shelf_fast_path_even_when_synthesis_endpoint_is_set() {
        let _lock = ENV_LOCK.lock().await;
        let (endpoint, seen, server) = start_synthesis_mock(
            "event: full\ndata: {\"should_not\":\"be called\"}\n\n",
            "text/event-stream",
        )
        .await;
        let _endpoint = EnvGuard::set("GLASS_SYNTHESIS_ENDPOINT", endpoint);
        let _shelf = EnvGuard::remove("GLASS_FLEET_RETRO_SHELF_URL");

        let text = report_text(
            "daily",
            WindowReportQuery {
                scope: Some("fleet".to_string()),
                since: Some("2026-07-07T00:00:00Z".to_string()),
                until: Some("2026-07-07T01:00:00Z".to_string()),
            },
        )
        .await;

        assert!(
            text.contains("event: skeleton") && text.contains("GLASS_FLEET_RETRO_SHELF_URL"),
            "daily must stay on the shelf path, not custom synthesis: {text}"
        );
        assert!(
            seen.lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .is_none(),
            "daily/weekly shelf windows must not call GLASS_SYNTHESIS_ENDPOINT"
        );
        server.abort();
    }

    #[tokio::test]
    async fn custom_window_posts_to_synthesis_endpoint_and_preserves_cite_nodes() {
        let _lock = ENV_LOCK.lock().await;
        let body = r#"event: partial
data: {"stage":"partial","text":"first token"}

event: full
data: {"stage":"full","spec":{"generated_at":"2026-07-07T01:00:00Z","components":[{"type":"hero","title":"Custom retro","summary":[{"type":"cite","text":"evidence survived","ref_id":"powder:glass-919"}],"stats":[],"image_intent":null}]}}

"#;
        let (endpoint, seen, server) = start_synthesis_mock(body, "text/event-stream").await;
        let _endpoint = EnvGuard::set("GLASS_SYNTHESIS_ENDPOINT", endpoint);
        let _shelf = EnvGuard::remove("GLASS_FLEET_RETRO_SHELF_URL");

        let text = report_text(
            "custom",
            WindowReportQuery {
                scope: Some("repo:glass".to_string()),
                since: Some("2026-07-07T00:00:00Z".to_string()),
                until: Some("2026-07-07T01:00:00Z".to_string()),
            },
        )
        .await;

        let payload = seen
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
            .expect("synthesis endpoint was called");
        assert_eq!(payload["window"], "custom");
        assert_eq!(payload["scope"], "repo:glass");
        assert_eq!(payload["since"], "2026-07-07T00:00:00Z");
        assert_eq!(payload["until"], "2026-07-07T01:00:00Z");
        assert!(text.contains("event: skeleton"));
        assert!(text.contains("event: partial"));
        assert!(text.contains("event: full"));
        assert!(
            text.contains(r#""type":"cite""#) && text.contains(r#""ref_id":"powder:glass-919""#),
            "Glass must relay citation metadata unchanged through the custom synthesis path: {text}"
        );
        server.abort();
    }
}
