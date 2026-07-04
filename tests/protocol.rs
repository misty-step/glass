use axum::body::Body;
use axum::http::{Request, StatusCode};
use glass::{
    CreateComment, DoctorConfig, Glass, NewSession, PublishPost, SURFACE_KINDS, Surface,
    SurfaceKind, app_router, run_doctor,
};
use http_body_util::BodyExt;
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tower::ServiceExt;

#[tokio::test]
async fn surface_kinds_match_the_frozen_sideshow_contract() {
    assert_eq!(
        SURFACE_KINDS,
        [
            SurfaceKind::Html,
            SurfaceKind::Diff,
            SurfaceKind::Image,
            SurfaceKind::Trace,
            SurfaceKind::Markdown,
            SurfaceKind::Terminal,
            SurfaceKind::Mermaid,
            SurfaceKind::Json,
            SurfaceKind::Code,
        ]
    );
}

#[tokio::test]
async fn wait_and_write_piggyback_share_one_exactly_once_feedback_cursor() {
    let glass = Glass::memory().expect("memory store");
    let session = glass
        .create_session(NewSession {
            agent: "codex-lane".into(),
            title: "protocol test".into(),
            cwd: Some("/tmp/glass".into()),
        })
        .expect("session");
    let post = glass
        .publish_post(PublishPost {
            session_id: Some(session.id.clone()),
            session_title: None,
            agent: None,
            title: "first post".into(),
            surfaces: vec![
                Surface::new(SurfaceKind::Markdown, json!({"markdown": "first"})).expect("surface"),
            ],
        })
        .expect("publish")
        .post;

    glass
        .create_comment(CreateComment {
            session_id: session.id.clone(),
            post_id: post.id.clone(),
            author: "user".into(),
            text: "change direction".into(),
        })
        .expect("comment");

    let first_wait = glass.wait_for_feedback(&session.id, 0).expect("first wait");
    assert_eq!(first_wait.len(), 1);
    assert_eq!(first_wait[0].text, "change direction");

    let second_wait = glass
        .wait_for_feedback(&session.id, 0)
        .expect("second wait");
    assert!(second_wait.is_empty(), "wait must not redeliver feedback");

    glass
        .create_comment(CreateComment {
            session_id: session.id.clone(),
            post_id: post.id.clone(),
            author: "user".into(),
            text: "piggyback this".into(),
        })
        .expect("second comment");

    let write = glass
        .update_post(
            &post.id,
            PublishPost {
                session_id: Some(session.id.clone()),
                session_title: None,
                agent: None,
                title: "updated post".into(),
                surfaces: vec![
                    Surface::new(SurfaceKind::Markdown, json!({"markdown": "second"}))
                        .expect("surface"),
                ],
            },
        )
        .expect("update");
    assert_eq!(write.user_feedback.len(), 1);
    assert_eq!(write.user_feedback[0].text, "piggyback this");

    let repeat_write = glass
        .update_post(
            &post.id,
            PublishPost {
                session_id: Some(session.id.clone()),
                session_title: None,
                agent: None,
                title: "updated post again".into(),
                surfaces: vec![
                    Surface::new(SurfaceKind::Markdown, json!({"markdown": "third"}))
                        .expect("surface"),
                ],
            },
        )
        .expect("repeat update");
    assert!(
        repeat_write.user_feedback.is_empty(),
        "piggyback must share the same cursor as wait"
    );
}

#[tokio::test]
async fn assets_are_content_addressed_by_sha256() {
    let glass = Glass::memory().expect("memory store");
    let first = glass
        .store_asset("image/png", Some("pixel.png"), b"same bytes")
        .expect("first asset");
    let second = glass
        .store_asset("image/png", Some("again.png"), b"same bytes")
        .expect("second asset");

    assert_eq!(first.id, second.id);
    assert_eq!(
        first.id,
        "58100dc8fc06562ce3e578231dc948e083520ee49c4b4ee5a5a28bb4b4003feb"
    );
    assert_eq!(first.byte_length, 10);
}

#[tokio::test]
async fn sandbox_render_route_carries_the_sandbox_in_the_response_header() {
    let glass = Glass::memory().expect("memory store");
    let session = glass
        .create_session(NewSession {
            agent: "codex-lane".into(),
            title: "sandbox test".into(),
            cwd: None,
        })
        .expect("session");
    let post = glass
        .publish_post(PublishPost {
            session_id: Some(session.id),
            session_title: None,
            agent: None,
            title: "html surface".into(),
            surfaces: vec![
                Surface::new(
                    SurfaceKind::Html,
                    json!({"html": "<button onclick=\"window.sendPrompt('go')\">go</button>"}),
                )
                .expect("surface"),
            ],
        })
        .expect("publish")
        .post;

    let response = app_router(glass)
        .oneshot(
            Request::builder()
                .uri(format!("/s/{}?part=0", post.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let csp = response
        .headers()
        .get("content-security-policy")
        .expect("csp")
        .to_str()
        .expect("csp text");
    assert!(csp.contains("sandbox"));
    assert!(!csp.contains("allow-same-origin"));

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("sendPrompt"));
}

#[tokio::test]
async fn mcp_tool_list_and_setup_docs_expose_agent_onboarding() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("mcp response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let tools = value["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "publish_post"));
    assert!(tools.iter().any(|tool| tool["name"] == "wait_for_feedback"));

    let setup = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/setup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("setup response");
    let body = setup.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("/agent-howto"));
    assert!(text.contains("/mcp"));
}

#[tokio::test]
async fn recent_posts_summarize_agents_once_from_session_metadata() {
    let glass = Glass::memory().expect("memory store");
    let lead_a = glass
        .create_session(NewSession {
            agent: "lead".into(),
            title: "lead daybook command center".into(),
            cwd: Some("/tmp/daybook-a".into()),
        })
        .expect("lead a");
    let lead_b = glass
        .create_session(NewSession {
            agent: "lead".into(),
            title: "lead daybook command center".into(),
            cwd: Some("/tmp/daybook-b".into()),
        })
        .expect("lead b");
    let critic = glass
        .create_session(NewSession {
            agent: "critic".into(),
            title: "critic lane".into(),
            cwd: None,
        })
        .expect("critic");

    for session in [&lead_a, &lead_b, &critic] {
        glass
            .publish_post(PublishPost {
                session_id: Some(session.id.clone()),
                session_title: None,
                agent: None,
                title: format!("{} post", session.agent),
                surfaces: vec![
                    Surface::new(SurfaceKind::Markdown, json!({"markdown": "proof"}))
                        .expect("surface"),
                ],
            })
            .expect("publish");
    }

    let response = app_router(glass)
        .oneshot(
            Request::builder()
                .uri("/api/posts/recent?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let agents = value["agents"].as_array().expect("agents");
    let lead_agents = agents
        .iter()
        .filter(|agent| agent["agent"] == "lead")
        .collect::<Vec<_>>();
    assert_eq!(lead_agents.len(), 1);
    assert_eq!(lead_agents[0]["postCount"], 2);
    assert_eq!(lead_agents[0]["sessionCount"], 2);
    assert_eq!(
        value["sessions"]
            .as_array()
            .expect("sessions")
            .iter()
            .filter(|session| session["agent"] == "lead")
            .count(),
        2
    );
}

#[tokio::test]
async fn doctor_verifies_running_service_db_backing_and_feedback_probe() {
    let db_path = temp_db_path("glass-doctor");
    let glass = Glass::open(&db_path).expect("db-backed glass");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        axum::serve(listener, app_router(glass))
            .await
            .expect("test server");
    });

    let report = run_doctor(DoctorConfig {
        url: format!("http://{addr}"),
        db_path: db_path.clone(),
        timeout: Duration::from_secs(2),
    })
    .await
    .expect("doctor report");

    assert_eq!(report.db_path, db_path);
    assert_eq!(report.feedback_text, "glass doctor feedback probe");
    assert!(report.probe_session_id.starts_with("ses-"));
    assert!(report.probe_post_id.starts_with("post-"));
    assert!(report.session_count >= 1);

    let reopened = Glass::open(&db_path).expect("reopen db");
    let sessions = reopened.list_sessions().expect("sessions");
    assert!(sessions.iter().any(|session| {
        session.id == report.probe_session_id && session.agent == "glass-doctor"
    }));

    server.abort();
    let _ = std::fs::remove_file(db_path);
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nonce}.db"))
}
