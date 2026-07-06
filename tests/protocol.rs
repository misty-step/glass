use axum::body::Body;
use axum::http::{Request, StatusCode};
use glass::{
    DoctorConfig, Glass, NewSession, PublishPost, SURFACE_KINDS, Surface, SurfaceKind, app_router,
    run_doctor,
};
use http_body_util::BodyExt;
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tower::ServiceExt;

#[tokio::test]
async fn surface_kinds_match_the_frozen_sideshow_contract_plus_metric() {
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
            SurfaceKind::Metric,
        ]
    );
}

#[tokio::test]
async fn publish_outcome_carries_no_comment_or_feedback_surface() {
    let glass = Glass::memory().expect("memory store");
    let outcome = glass
        .publish_post(PublishPost {
            session_id: None,
            session_title: None,
            agent: None,
            title: "one-way post".into(),
            surfaces: vec![
                Surface::new(SurfaceKind::Markdown, json!({"markdown": "one-way"}))
                    .expect("surface"),
            ],
        })
        .expect("publish");
    let value = serde_json::to_value(&outcome).expect("serialize outcome");
    assert!(
        value.get("userFeedback").is_none() && value.get("user_feedback").is_none(),
        "glass-912 deleted the feedback surface: {value}"
    );
}

#[tokio::test]
async fn metric_surface_requires_label_and_value() {
    let missing_value = Surface::new(SurfaceKind::Metric, json!({"label": "tests"}));
    assert!(missing_value.is_err(), "metric without value must reject");

    let missing_label = Surface::new(SurfaceKind::Metric, json!({"value": "42 passed"}));
    assert!(missing_label.is_err(), "metric without label must reject");

    let metric = Surface::new(
        SurfaceKind::Metric,
        json!({"label": "tests", "value": "42 passed"}),
    )
    .expect("valid metric surface");
    assert_eq!(metric.kind, SurfaceKind::Metric);
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
    assert!(
        !tools.iter().any(|tool| tool["name"] == "wait_for_feedback"),
        "glass-912 deleted the feedback tool: {tools:?}"
    );

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
async fn doctor_verifies_running_service_db_backing_and_self_cleans() {
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
    assert!(report.probe_session_id.starts_with("ses-"));
    assert!(report.probe_post_id.starts_with("post-"));
    assert!(report.session_count >= 1);

    // The doctor proves the round trip through a fresh reopen internally
    // (it would have bailed above had persistence failed), then self-cleans
    // its probe so diagnostic exhaust doesn't linger in the stage.
    let reopened = Glass::open(&db_path).expect("reopen db");
    let sessions = reopened.list_sessions().expect("sessions");
    assert!(
        !sessions
            .iter()
            .any(|session| session.id == report.probe_session_id),
        "doctor probe session must self-clean after verifying the round trip"
    );
    let posts = reopened
        .list_recent_posts(50)
        .expect("posts after doctor run");
    assert!(
        !posts.iter().any(|post| post.id == report.probe_post_id),
        "doctor probe post must self-clean after verifying the round trip"
    );

    server.abort();
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn recent_posts_excludes_glass_doctor_probes_from_the_operator_stream() {
    let glass = Glass::memory().expect("memory store");
    let operator_session = glass
        .create_session(NewSession {
            agent: "codex-lane".into(),
            title: "real work".into(),
            cwd: Some("/tmp/glass-operator".into()),
        })
        .expect("operator session");
    glass
        .publish_post(PublishPost {
            session_id: Some(operator_session.id.clone()),
            session_title: None,
            agent: None,
            title: "operator content".into(),
            surfaces: vec![
                Surface::new(SurfaceKind::Markdown, json!({"markdown": "real"})).expect("surface"),
            ],
        })
        .expect("publish operator post");

    let probe_session = glass
        .create_session(NewSession {
            agent: "glass-doctor".into(),
            title: "glass doctor probe".into(),
            cwd: None,
        })
        .expect("probe session");
    glass
        .publish_post(PublishPost {
            session_id: Some(probe_session.id.clone()),
            session_title: None,
            agent: None,
            title: "Glass doctor probe".into(),
            surfaces: vec![
                Surface::new(
                    SurfaceKind::Markdown,
                    json!({"markdown": "Glass doctor disposable probe."}),
                )
                .expect("surface"),
            ],
        })
        .expect("publish probe post");

    let response = app_router(glass)
        .oneshot(
            Request::builder()
                .uri("/api/posts/recent?limit=50")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let posts = value["posts"].as_array().expect("posts");
    assert!(posts.iter().any(|post| post["title"] == "operator content"));
    assert!(
        !posts
            .iter()
            .any(|post| post["session_id"] == probe_session.id),
        "doctor probe posts must not appear in the operator stream"
    );

    let sessions = value["sessions"].as_array().expect("sessions");
    assert!(
        !sessions
            .iter()
            .any(|session| session["id"] == probe_session.id),
        "doctor probe sessions must not appear in the operator stream"
    );

    let agents = value["agents"].as_array().expect("agents");
    assert!(
        !agents.iter().any(|agent| agent["agent"] == "glass-doctor"),
        "glass-doctor must not appear as an agent in the operator stream"
    );
}

#[tokio::test]
async fn dead_sessions_are_flagged_live_false_but_keep_their_posts() {
    let db_path = temp_db_path("glass-dead");
    let glass = Glass::open(&db_path).expect("db-backed glass");
    let session = glass
        .create_session(NewSession {
            agent: "codex-lane".into(),
            title: "long quiet lane".into(),
            cwd: None,
        })
        .expect("session");
    glass
        .publish_post(PublishPost {
            session_id: Some(session.id.clone()),
            session_title: None,
            agent: None,
            title: "last known status".into(),
            surfaces: vec![
                Surface::new(SurfaceKind::Markdown, json!({"markdown": "quiet"})).expect("surface"),
            ],
        })
        .expect("publish");

    // Backdate the session well past LIVE_WINDOW_SECONDS. There is no public
    // setter for this (by design), so open a second connection to the same
    // file, exactly as a second concurrent agent process would.
    {
        let conn = rusqlite::Connection::open(&db_path).expect("second connection");
        conn.execute(
            "UPDATE sessions SET last_active_at = last_active_at - 3600 WHERE id = ?1",
            [&session.id],
        )
        .expect("backdate session");
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
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let sessions = value["sessions"].as_array().expect("sessions");
    let backdated = sessions
        .iter()
        .find(|entry| entry["id"] == session.id)
        .expect("backdated session present");
    assert_eq!(
        backdated["isLive"], false,
        "a session quiet for over LIVE_WINDOW_SECONDS must report isLive=false: {backdated}"
    );

    // Dead sessions demote out of the primary rail client-side; the API
    // keeps serving their full post history rather than dropping it.
    let posts = value["posts"].as_array().expect("posts");
    assert!(posts.iter().any(|post| post["session_id"] == session.id));

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn recent_posts_agent_filter_scopes_posts_but_not_the_roster() {
    let glass = Glass::memory().expect("memory store");
    let alpha = glass
        .create_session(NewSession {
            agent: "alpha".into(),
            title: "alpha lane".into(),
            cwd: None,
        })
        .expect("alpha session");
    let beta = glass
        .create_session(NewSession {
            agent: "beta".into(),
            title: "beta lane".into(),
            cwd: None,
        })
        .expect("beta session");
    for (session, text) in [(&alpha, "alpha status"), (&beta, "beta status")] {
        glass
            .publish_post(PublishPost {
                session_id: Some(session.id.clone()),
                session_title: None,
                agent: None,
                title: text.to_string(),
                surfaces: vec![
                    Surface::new(SurfaceKind::Markdown, json!({"markdown": text}))
                        .expect("surface"),
                ],
            })
            .expect("publish");
    }

    let response = app_router(glass)
        .oneshot(
            Request::builder()
                .uri("/api/posts/recent?agent=alpha")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let posts = value["posts"].as_array().expect("posts");
    assert_eq!(posts.len(), 1, "agent filter must scope posts: {posts:?}");
    assert_eq!(posts[0]["title"], "alpha status");

    // The rail needs the full roster regardless of which agent's feed is
    // open, so a viewer can navigate to any other agent from any view.
    let agents = value["agents"].as_array().expect("agents");
    assert!(agents.iter().any(|agent| agent["agent"] == "alpha"));
    assert!(agents.iter().any(|agent| agent["agent"] == "beta"));
}

#[tokio::test]
async fn aesthetic_css_is_served_for_the_shell_and_sandboxed_surfaces() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/aesthetic.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .expect("content-type")
        .to_str()
        .expect("content-type text");
    assert!(content_type.contains("text/css"));
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let css = String::from_utf8(body.to_vec()).unwrap();
    assert!(css.contains("--ae-accent"));
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nonce}.db"))
}
