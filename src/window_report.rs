//! REP-1's data seam (glass-917): Glass becomes the fourth consumer of the
//! fleet-retro/weave synthesis spine instead of building a second one.
//!
//! fleet-retro is a CLI-only nightly/weekly batch job (no live HTTP synthesis
//! API exists upstream today -- see glass-917's card history for the
//! live-repo check that corrected the original premise). What DOES exist is
//! the shelf-published `spec.json` fleet-retro writes after every run, at
//! `<shelf-base>/<window>/spec.json`. This module fetches that artifact,
//! caches it per (window, scope) with single-flight coalescing, and streams
//! it to the caller as two SSE events: an instant `skeleton` (no network
//! wait) followed by `full` once the (possibly cached) fetch resolves.
//! Custom/arbitrary windows and literal token streaming are out of scope --
//! carried forward on glass-919, which touches weave, not glass.

use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::{Path as AxumPath, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::Deserialize;
use serde_json::{Value, json};

/// fleet-retro currently only produces these two windows on a schedule.
/// Arbitrary/custom windows need an on-demand synthesis service that does
/// not exist yet (glass-919).
const SUPPORTED_WINDOWS: [&str; 2] = ["daily", "weekly"];

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

async fn fetch_from_shelf(window: &str) -> Result<Value, String> {
    let base = shelf_base_url()
        .ok_or_else(|| "GLASS_FLEET_RETRO_SHELF_URL is not configured".to_string())?;
    let url = format!("{}/{window}/spec.json", base.trim_end_matches('/'));
    let response = reqwest::get(&url)
        .await
        .map_err(|err| format!("fetch {url}: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "fetch {url}: upstream returned {}",
            response.status()
        ));
    }
    response
        .json::<Value>()
        .await
        .map_err(|err| format!("parse {url}: {err}"))
}

#[derive(Debug, Deserialize)]
pub struct WindowReportQuery {
    scope: Option<String>,
}

/// `GET /api/window-report/{window}` (window = `daily` | `weekly`).
/// Streams an instant `skeleton` event, then a `full` event carrying the
/// entire fetched (or cached) fleet-retro spec.json unchanged -- citation
/// metadata is never touched, only passed through.
pub async fn window_report(
    AxumPath(window): AxumPath<String>,
    Query(params): Query<WindowReportQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    if !SUPPORTED_WINDOWS.contains(&window.as_str()) {
        return Err(StatusCode::NOT_FOUND);
    }
    let scope = params.scope.unwrap_or_else(|| "fleet".to_string());
    let cache = global_cache();

    let stream = async_stream::stream! {
        yield Ok::<_, Infallible>(
            Event::default()
                .event("skeleton")
                .data(json!({"stage": "skeleton", "window": window, "scope": scope}).to_string()),
        );

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
    use std::sync::atomic::{AtomicUsize, Ordering};

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
}
