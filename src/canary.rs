//! Env-gated, in-process Canary reporting for Glass.
//!
//! Missing credentials make every public function a silent no-op. When
//! credentials exist, sends leave the request path on bounded std threads with
//! short timeouts and swallowed failures.

use std::any::Any;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::Response;
use serde_json::{Value, json};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

const SERVICE: &str = "glass";
const MONITOR: &str = "glass";
const CHECKIN_INTERVAL: Duration = Duration::from_secs(60);
const TTL_MS: u64 = 120_000;
const SEND_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_IN_FLIGHT: usize = 16;
const MAX_MESSAGE_LEN: usize = 4096;

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
struct Config {
    endpoint: String,
    api_key: String,
    service: String,
    environment: String,
}

impl Config {
    fn from_env() -> Option<Self> {
        Self::from_parts(
            std::env::var("CANARY_ENDPOINT").ok(),
            std::env::var("CANARY_API_KEY")
                .or_else(|_| std::env::var("CANARY_INGEST_KEY"))
                .ok(),
            std::env::var("CANARY_SERVICE").ok(),
            std::env::var("CANARY_ENVIRONMENT").ok(),
        )
    }

    fn from_parts(
        endpoint: Option<String>,
        api_key: Option<String>,
        service: Option<String>,
        environment: Option<String>,
    ) -> Option<Self> {
        let endpoint = endpoint?.trim().trim_end_matches('/').to_string();
        let api_key = api_key?.trim().to_string();
        if endpoint.is_empty() || api_key.is_empty() {
            return None;
        }
        Some(Self {
            endpoint,
            api_key,
            service: service
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| SERVICE.to_string()),
            environment: environment
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "production".to_string()),
        })
    }
}

fn service() -> String {
    std::env::var("CANARY_SERVICE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| SERVICE.to_string())
}

pub fn report_error(error_class: &str, message: &str) {
    let Some(config) = Config::from_env() else {
        return;
    };
    let body = error_body(&config, error_class, message);
    spawn_send(config, "/api/v1/errors", body);
}

fn report_error_blocking(error_class: &str, message: &str) {
    let Some(config) = Config::from_env() else {
        return;
    };
    let body = error_body(&config, error_class, message);
    send_with_retry(&config, "/api/v1/errors", &body);
}

pub fn check_in() {
    let Some(config) = Config::from_env() else {
        return;
    };
    let body = json!({
        "monitor": MONITOR,
        "status": "alive",
        "summary": "glass heartbeat",
        "ttl_ms": TTL_MS,
    });
    spawn_send(config, "/api/v1/check-ins", body);
}

pub fn start_health_loop() {
    if Config::from_env().is_none() {
        return;
    }
    check_in();
    let _ = std::thread::Builder::new()
        .name("glass-canary-health".to_string())
        .spawn(|| {
            loop {
                std::thread::sleep(CHECKIN_INTERVAL);
                check_in();
            }
        });
}

pub fn install_panic_hook() {
    if Config::from_env().is_none() {
        return;
    }
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        report_error_blocking("glass.panic", &panic_hook_shape(info.location()));
        default(info);
    }));
}

pub fn panic_response(err: Box<dyn Any + Send + 'static>) -> Response {
    report_error(
        "glass.panic",
        &format!("route=axum error_kind={}", panic_payload_kind(&*err)),
    );
    let body = json!({ "error": "internal server error" }).to_string();
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn error_body(config: &Config, error_class: &str, message: &str) -> Value {
    json!({
        "service": config.service,
        "error_class": error_class,
        "message": truncate(&redact(message), MAX_MESSAGE_LEN),
        "severity": "error",
        "environment": config.environment,
        "fingerprint": [error_class],
    })
}

fn spawn_send(config: Config, path: &'static str, body: Value) {
    let Some(slot) = InFlightSlot::try_acquire() else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("glass-canary-report".to_string())
        .spawn(move || {
            let _slot = slot;
            send_with_retry(&config, path, &body);
        });
    drop(spawned);
}

fn send_with_retry(config: &Config, path: &str, body: &Value) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(SEND_TIMEOUT))
        .build()
        .into();
    let url = format!("{}{}", config.endpoint, path);
    let authorization = format!("Bearer {}", config.api_key);
    for _ in 0..2 {
        let sent = agent
            .post(&url)
            .header("Authorization", &authorization)
            .send_json(body)
            .is_ok();
        if sent {
            break;
        }
    }
}

struct InFlightSlot;

impl InFlightSlot {
    fn try_acquire() -> Option<Self> {
        if IN_FLIGHT.fetch_add(1, Ordering::SeqCst) >= MAX_IN_FLIGHT {
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self)
    }
}

impl Drop for InFlightSlot {
    fn drop(&mut self) {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn drain(deadline: Duration) {
    let started = Instant::now();
    while IN_FLIGHT.load(Ordering::SeqCst) > 0 && started.elapsed() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub struct CanaryLayer;

impl<S: Subscriber> Layer<S> for CanaryLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if Config::from_env().is_none() || *event.metadata().level() != Level::ERROR {
            return;
        }
        if is_transport_target(event.metadata().target()) {
            return;
        }
        let mut message = String::new();
        event.record(&mut Visitor(&mut message));
        let class = format!("{}.{}", service(), event.metadata().target());
        report_error(&class, &message);
    }
}

struct Visitor<'a>(&'a mut String);

impl tracing::field::Visit for Visitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        if is_sensitive_field(field.name()) {
            self.0.push_str(field.name());
            self.0.push_str("=<redacted>");
        } else {
            self.0.push_str(field.name());
            self.0.push('=');
            self.0.push_str(&redact(&format!("{value:?}")));
        }
    }
}

fn is_transport_target(target: &str) -> bool {
    [
        "hyper",
        "h2",
        "reqwest",
        "rustls",
        "tokio_rustls",
        "tower",
        "tower_http",
        "ureq",
        "ureq_proto",
    ]
    .iter()
    .any(|prefix| target == *prefix || target.starts_with(&format!("{prefix}::")))
}

fn panic_hook_shape(location: Option<&std::panic::Location<'_>>) -> String {
    let location = location
        .map(|loc| format!("{}:{}", loc.file(), loc.line()))
        .unwrap_or_else(|| "unknown".to_string());
    format!("route=process error_kind=panic location={location}")
}

fn panic_payload_kind(payload: &(dyn Any + Send + 'static)) -> &'static str {
    if payload.is::<String>() {
        "panic_string"
    } else if payload.is::<&'static str>() {
        "panic_str"
    } else {
        "panic"
    }
}

fn truncate(message: &str, max_len: usize) -> String {
    message.chars().take(max_len).collect()
}

pub fn redact(raw: &str) -> String {
    let mut redact_next = false;
    raw.split_whitespace()
        .map(|word| {
            let redacted = redact_word(word, redact_next);
            redact_next = word_is_bare_sensitive_key(word) || word.eq_ignore_ascii_case("bearer");
            redacted
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_word(word: &str, redact_because_previous: bool) -> String {
    if redact_because_previous || looks_like_secret(word) {
        return "<redacted>".to_string();
    }
    let Some((prefix, _)) = word.split_once(['=', ':']) else {
        return word.to_string();
    };
    if is_sensitive_field(prefix.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_'))
    {
        let separator = if word.contains('=') { '=' } else { ':' };
        return format!("{prefix}{separator}<redacted>");
    }
    word.to_string()
}

fn word_is_bare_sensitive_key(word: &str) -> bool {
    let trimmed = word.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_');
    is_sensitive_field(trimmed)
}

fn is_sensitive_field(field: &str) -> bool {
    let lower = field.to_ascii_lowercase();
    [
        "authorization",
        "body",
        "cookie",
        "credential",
        "password",
        "secret",
        "token",
        "api_key",
        "apikey",
        "key",
    ]
    .iter()
    .any(|needle| lower == *needle || lower.ends_with(&format!("_{needle}")))
}

fn looks_like_secret(word: &str) -> bool {
    let trimmed = word.trim_matches(|ch: char| {
        ch == '"' || ch == '\'' || ch == ',' || ch == ';' || ch == ')' || ch == '('
    });
    trimmed.starts_with("sk_")
        || trimmed.starts_with("sk-")
        || trimmed.starts_with("op://")
        || trimmed.starts_with("ghp_")
        || trimmed.starts_with("github_pat_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Mutex, OnceLock, mpsc};
    use tracing_subscriber::prelude::*;

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn config_is_none_without_endpoint_or_key() {
        assert!(Config::from_parts(None, Some("k".into()), None, None).is_none());
        assert!(Config::from_parts(Some("http://x".into()), None, None, None).is_none());
        assert!(
            Config::from_parts(Some("http://x".into()), Some("k".into()), None, None).is_some()
        );
    }

    #[test]
    fn posts_error_and_check_in_contracts() {
        let (endpoint, requests) = serve_n(2);
        let _guard = env_guard(&endpoint);

        report_error(
            "glass.test.failed",
            "route=/test error_kind=storage token=sk_live_secret",
        );
        check_in();
        drain(Duration::from_secs(2));

        let first = requests
            .recv_timeout(Duration::from_secs(2))
            .expect("first request");
        let second = requests
            .recv_timeout(Duration::from_secs(2))
            .expect("second request");
        let joined = format!("{first}\n{second}");
        let payloads = [body_json(&first), body_json(&second)];
        assert!(joined.contains("POST /api/v1/errors"));
        assert!(joined.contains("POST /api/v1/check-ins"));
        assert!(joined.contains("Bearer sk_test_key"));
        assert!(payloads.iter().any(|payload| payload["service"] == "glass"));
        assert!(payloads.iter().any(|payload| payload["monitor"] == "glass"));
        assert!(!joined.contains("sk_live_secret"));
    }

    #[test]
    fn reporting_failures_never_reach_the_caller() {
        let _guard = env_guard("http://127.0.0.1:9");
        report_error("glass.test.failed", "route=/test error_kind=dead_port");
        drain(Duration::from_secs(3));
    }

    #[test]
    fn tracing_layer_redacts_secret_fields_and_message_tokens() {
        let (endpoint, requests) = serve_n(1);
        let _guard = env_guard(&endpoint);
        let subscriber = tracing_subscriber::registry().with(CanaryLayer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::error!(
                target: "glass::tests",
                api_key = "sk_live_should_not_leave",
                body = "{\"credential\":\"also-secret\"}",
                "failed with Authorization: Bearer sk_other_secret"
            );
        });
        drain(Duration::from_secs(2));

        let request = requests
            .recv_timeout(Duration::from_secs(2))
            .expect("request");
        assert!(request.contains("POST /api/v1/errors"));
        assert!(!request.contains("sk_live_should_not_leave"));
        assert!(!request.contains("also-secret"));
        assert!(!request.contains("sk_other_secret"));
        let payload = request.split("\r\n\r\n").nth(1).expect("body");
        assert!(payload.contains("<redacted>"));
    }

    #[test]
    fn panic_response_reports_shape_without_payload() {
        let (endpoint, requests) = serve_n(1);
        let _guard = env_guard(&endpoint);

        let response = panic_response(Box::new("password=hunter2 body={secret}".to_string()));
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        drain(Duration::from_secs(2));

        let request = requests
            .recv_timeout(Duration::from_secs(2))
            .expect("request");
        assert!(request.contains("POST /api/v1/errors"));
        assert!(request.contains("glass.panic"));
        assert!(!request.contains("hunter2"));
        assert!(!request.contains("{secret}"));
    }

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("CANARY_ENDPOINT");
                std::env::remove_var("CANARY_API_KEY");
                std::env::remove_var("CANARY_INGEST_KEY");
                std::env::remove_var("CANARY_SERVICE");
                std::env::remove_var("CANARY_ENVIRONMENT");
            }
        }
    }

    fn env_guard(endpoint: &str) -> EnvGuard {
        let lock = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        unsafe {
            std::env::set_var("CANARY_ENDPOINT", endpoint);
            std::env::set_var("CANARY_API_KEY", "sk_test_key");
            std::env::set_var("CANARY_ENVIRONMENT", "test");
            std::env::remove_var("CANARY_INGEST_KEY");
        }
        EnvGuard { _lock: lock }
    }

    fn body_json(request: &str) -> serde_json::Value {
        serde_json::from_str(request.split("\r\n\r\n").nth(1).expect("body")).expect("json")
    }

    fn serve_n(count: usize) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for _ in 0..count {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut request = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    let read = stream.read(&mut buffer).expect("read");
                    request.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some(header_end) = text.find("\r\n\r\n") {
                        let content_length = text
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|value| value.trim().parse::<usize>().expect("length"))
                            })
                            .unwrap_or(0);
                        if request.len() >= header_end + 4 + content_length {
                            break;
                        }
                    }
                    if read == 0 {
                        break;
                    }
                }
                sender
                    .send(String::from_utf8_lossy(&request).into_owned())
                    .expect("send request");
                let response =
                    "HTTP/1.1 201 Created\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}";
                stream.write_all(response.as_bytes()).expect("write");
            }
        });

        (format!("http://{address}"), receiver)
    }
}
