use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::{Json as AxumJson, Router};
use chrono::{Duration as ChronoDuration, Local};
use glass::{
    DoctorConfig, Glass, NewSession, PublishPost, SURFACE_KINDS, Surface, SurfaceKind, app_router,
    run_doctor,
};
use http_body_util::BodyExt;
use serde_json::json;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tower::ServiceExt;

fn assert_shared_rail(html: &str, active_href: Option<&str>) {
    assert_eq!(
        html.matches(
            r#"<aside id="glass-rail" class="ae-rail glass-rail" aria-label="Glass places">"#
        )
        .count(),
        1,
        "every human page must render exactly one shared rail: {html}"
    );
    assert!(html.contains(r#"<p class="ae-h">PLACES</p>"#));
    assert!(html.contains(r#"href="/""#) && html.contains(">Now</span>"));
    assert!(
        html.contains(r#"href="/needs-you""#) && html.contains(">Needs you</span>"),
        "when Powder is unavailable, the needs-you place must degrade without breaking the link: {html}"
    );
    assert!(html.contains(r#"href="/reports""#) && html.contains(">Reports</span>"));
    assert!(
        !html.contains(r#"href="/clips""#),
        "Clips is retired as a rail place: {html}"
    );
    assert!(html.contains("data-sanctum-home"));
    assert!(html.contains(r#"href="/setup""#) && html.contains(">Wire an agent</span>"));
    assert!(html.contains(r#"<button class="ae-mode" type="button""#));

    let current_count = html.matches(r#"aria-current="page""#).count();
    if let Some(href) = active_href {
        assert!(
            html.contains(&format!(r#"href="{href}" aria-current="page""#)),
            "active place {href} is missing aria-current: {html}"
        );
        assert_eq!(current_count, 1, "only one place may be active: {html}");
    } else {
        assert_eq!(
            current_count, 0,
            "routes outside declared places should not mark a rail place active: {html}"
        );
    }
}

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
    assert!(tools.iter().any(|tool| tool["name"] == "capture_clip"));
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
async fn clip_capture_records_a_review_queue_item_with_context_and_caption() {
    let glass = Glass::memory().expect("memory store");
    let session = glass
        .create_session(NewSession {
            agent: "codex-lane".into(),
            title: "clip lane".into(),
            cwd: Some("/tmp/glass-clip".into()),
        })
        .expect("session");
    let post = glass
        .publish_post(PublishPost {
            session_id: Some(session.id.clone()),
            session_title: None,
            agent: None,
            title: "surprising output".into(),
            surfaces: vec![
                Surface::new(SurfaceKind::Markdown, json!({"markdown": "this mattered"}))
                    .expect("surface"),
            ],
        })
        .expect("publish")
        .post;
    let app = app_router(glass);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/clips")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "session_id": session.id,
                        "post_id": post.id,
                        "surface_index": 0,
                        "range": { "start": 0, "end": 30 },
                        "note": "clip this for review"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("clip response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let captured: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(captured["clip"]["post_version"], 1);
    assert_eq!(captured["clip"]["surface_index"], 0);
    assert_eq!(captured["context"]["session"]["agent"], "codex-lane");
    assert_eq!(captured["context"]["post"]["title"], "surprising output");
    assert_eq!(captured["context"]["surface"]["kind"], "markdown");
    assert!(
        captured["draft_caption"]
            .as_str()
            .unwrap()
            .contains("clip this for review")
    );
    assert!(
        captured["context"]["evidence_links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|link| link["url"].as_str().unwrap().starts_with("/session/"))
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/clips?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("queue response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let queue: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let clips = queue["clips"].as_array().expect("clips");
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0]["clip"]["note"], "clip this for review");
    assert_eq!(clips[0]["context"]["surface"]["id"], "surface-1");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clips")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("clips redirect response");
    assert_eq!(response.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        response
            .headers()
            .get("location")
            .expect("location")
            .to_str()
            .expect("location text"),
        "/"
    );
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
async fn recent_feed_projects_glass_posts_as_bridge_shaped_events() {
    let glass = Glass::memory().expect("memory store");
    let session = glass
        .create_session(NewSession {
            agent: "codex-glass".into(),
            title: "glass-926 Wire".into(),
            cwd: None,
        })
        .expect("session");
    glass
        .publish_post(PublishPost {
            session_id: Some(session.id.clone()),
            session_title: None,
            agent: None,
            title: "Default feed shipped".into(),
            surfaces: vec![
                Surface::new(
                    SurfaceKind::Markdown,
                    json!({
                        "markdown": "The Wire reads Glass posts.",
                        "feedKind": "shipped",
                        "summary": "The Wire is backed by the native post store.",
                        "evidenceLinks": [{"label": "PR", "url": "https://github.com/misty-step/glass/pull/926"}]
                    }),
                )
                .expect("surface"),
            ],
        })
        .expect("publish");

    let response = app_router(glass)
        .oneshot(
            Request::builder()
                .uri("/api/feed/recent?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let events = value["events"].as_array().expect("events");
    let event = events
        .iter()
        .find(|event| event["title"] == "Default feed shipped")
        .expect("post event");
    assert_eq!(event["kind"], "shipped");
    assert_eq!(event["source"], "glass-posts");
    assert_eq!(event["agent"], "codex-glass");
    assert!(
        event["evidenceLinks"]
            .as_array()
            .expect("evidence links")
            .iter()
            .any(|link| link["url"] == "https://github.com/misty-step/glass/pull/926")
    );
    assert!(
        event["evidenceLinks"]
            .as_array()
            .expect("evidence links")
            .iter()
            .any(|link| link["url"]
                .as_str()
                .is_some_and(|url| url.starts_with("/s/"))),
        "sandboxed surface URL is a native evidence link: {event}"
    );
}

#[tokio::test]
async fn default_view_loads_the_now_endpoint_and_keeps_drilldowns_reachable() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("THE FLEET"));
    assert!(html.contains("THE WIRE"));
    assert!(html.contains("/api/now"));
    assert!(html.contains("/agent/"));
    assert!(html.contains("/session/"));
    assert!(html.contains("feed-dialog"));
    assert!(
        !html.contains("wait_for_feedback") && !html.contains("reply_to_user"),
        "the default feed must stay one-way"
    );
}

#[tokio::test]
async fn now_endpoint_renders_live_posts_in_the_wall_and_wire() {
    let glass = Glass::memory().expect("memory store");
    let session = glass
        .create_session(NewSession {
            agent: "now-agent".into(),
            title: "now lane".into(),
            cwd: None,
        })
        .expect("session");
    glass
        .publish_post(PublishPost {
            session_id: Some(session.id),
            session_title: None,
            agent: None,
            title: "Now proof".into(),
            surfaces: vec![
                Surface::new(
                    SurfaceKind::Markdown,
                    json!({"markdown": "proof", "feedKind": "report", "summary": "live post survives Powder down"}),
                )
                .expect("surface"),
            ],
        })
        .expect("publish");

    let response = app_router(glass)
        .oneshot(
            Request::builder()
                .uri("/api/now")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        value["stats"]["agentsLive"].as_u64().unwrap_or_default() >= 1,
        "at least the local live Glass agent should be on stage: {value}"
    );
    let wall = value["wall"].as_array().expect("wall");
    let card = wall
        .iter()
        .find(|card| card["agent"] == "now-agent")
        .expect("now-agent wall card");
    assert_eq!(card["meta"], "report: live post survives Powder down");
    assert!(
        value["wire"]
            .as_array()
            .expect("wire")
            .iter()
            .any(|event| event["title"] == "Now proof")
    );
}

#[tokio::test]
async fn recent_feed_merges_configured_landmark_release_events() {
    let _guard = landmark_env_lock().lock().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind landmark fixture");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        let app = Router::new().route(
            "/events",
            get(|| async {
                AxumJson(json!({
                    "events": [{
                        "id": "landmark-glass-v0.2.0",
                        "repo": "glass",
                        "version": "v0.2.0",
                        "title": "glass v0.2.0",
                        "summary": "Release notes published by Landmark.",
                        "created_at": 2_000_000_000_i64,
                        "url": "https://github.com/misty-step/glass/releases/tag/v0.2.0"
                    }]
                }))
            }),
        );
        axum::serve(listener, app).await.expect("landmark fixture");
    });
    let _env = EnvVarGuard::set(
        "GLASS_LANDMARK_RELEASE_EVENTS_URL",
        &format!("http://{addr}/events"),
    );

    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/api/feed/recent?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    server.abort();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["landmark"]["status"], "ok");
    let events = value["events"].as_array().expect("events");
    let release = events
        .iter()
        .find(|event| event["kind"] == "release")
        .expect("release event");
    assert_eq!(release["title"], "glass v0.2.0");
    assert_eq!(release["source"], "landmark");
    assert!(
        release["evidenceLinks"]
            .as_array()
            .expect("evidence links")
            .iter()
            .any(|link| link["url"] == "https://github.com/misty-step/glass/releases/tag/v0.2.0")
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

    // The viewer needs the full roster regardless of which agent's feed is
    // open, so the wall/drill-down state can stay in sync from any view.
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

#[tokio::test]
async fn viewer_carries_the_cross_repo_sanctum_home_affordance() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert_shared_rail(&html, Some("/"));
    assert!(
        html.contains("data-sanctum-home"),
        "viewer must carry the data-sanctum-home marker other Sanctum tooling scans for"
    );
    assert!(
        !html.contains("{{SANCTUM_URL}}"),
        "the SANCTUM_URL placeholder must be substituted, never leaked into served HTML"
    );
    assert!(
        html.contains(r#"href="/""#),
        "with no GLASS_SANCTUM_URL configured the affordance falls back to an inert \
         same-origin link rather than a hardcoded personal tailnet host (glass-915: this \
         repo is public and forkable); deployments behind a real Sanctum portal set \
         GLASS_SANCTUM_URL to the absolute portal root so cross-origin destinations still \
         resolve correctly (bastion-917 audit: unlike bastion's own same-origin injection, \
         Glass is a cross-origin destination)"
    );
}

#[test]
fn sanctum_url_from_honors_configured_override_and_falls_back_to_same_origin() {
    assert_eq!(
        glass::sanctum_url_from(Some("https://portal.example/".to_string())),
        "https://portal.example/"
    );
    assert_eq!(glass::sanctum_url_from(None), "/");
}

#[tokio::test]
async fn window_report_custom_windows_degrade_to_clear_fallback_events_when_unconfigured() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/api/window-report/custom?since=2026-07-07T00:00:00Z&until=2026-07-07T01:00:00Z&scope=fleet")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("event: skeleton"));
    assert!(
        text.contains("event: fallback") && text.contains("GLASS_SYNTHESIS_ENDPOINT"),
        "custom windows must name the missing synthesis endpoint instead of 404ing or \
         silently rendering blank: {text}"
    );
    assert!(
        text.contains("event: error") && text.contains("GLASS_FLEET_RETRO_SHELF_URL"),
        "with no shelf configured, the fallback must also fail loudly with the missing env \
         var named: {text}"
    );
}

#[tokio::test]
async fn window_report_streams_a_skeleton_before_the_shelf_fetch_resolves() {
    // No GLASS_FLEET_RETRO_SHELF_URL configured in the test process: the
    // shelf fetch deterministically fails fast with a clear message, so this
    // exercises the real skeleton-then-resolved-event streaming contract
    // without depending on network access or a live shelf.
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/api/window-report/daily")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(content_type.contains("text/event-stream"));
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains("event: skeleton"),
        "the skeleton event must be emitted before the shelf fetch resolves: {text}"
    );
    assert!(
        text.contains("event: error") && text.contains("GLASS_FLEET_RETRO_SHELF_URL"),
        "an unconfigured shelf must fail loudly with the missing env var named, not \
         silently: {text}"
    );
}

#[tokio::test]
async fn reports_shell_serves_the_sentence_builder_without_library() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/reports")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert_shared_rail(&html, Some("/reports"));
    assert!(html.contains("REPORT QUERY"));
    assert!(html.contains("Show me"));
    assert!(html.contains("the whole fleet"));
    assert!(html.contains("the past 24h"));
    assert!(html.contains(r#"id="reports-result""#));
    assert!(!html.contains("PLATE 1 - THE LIBRARY"));
    assert!(!html.contains("reports-library"));
}

#[tokio::test]
async fn rep1_human_route_redirects_to_reports_generator() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(Request::builder().uri("/rep1").body(Body::empty()).unwrap())
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::MOVED_PERMANENTLY);
    let location = response
        .headers()
        .get(axum::http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        location.starts_with("/reports"),
        "legacy /rep1 should fold into the reports page: {location}"
    );
}

#[tokio::test]
async fn rep1_report_rejects_windows_outside_the_locked_lab3_tab_set() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/api/rep1/monthly")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rep1_report_streams_a_glass_919_pending_error_for_non_live_windows() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/api/rep1/30m")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("event: skeleton"));
    assert!(
        text.contains("event: error") && text.contains("glass-919"),
        "30m has no shelf window to map to; it must fail loudly naming glass-919, not \
         silently render nothing: {text}"
    );
}

#[tokio::test]
async fn backlog_human_route_redirects_to_reports_generator_with_repo_scope() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/backlog/glass")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::MOVED_PERMANENTLY);
    let location = response
        .headers()
        .get(axum::http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert_eq!(location, "/reports?kind=backlog&scope=repo%3Aglass");
}

#[tokio::test]
async fn activity_report_generation_persists_and_reopens_by_stable_url() {
    let glass = Glass::memory().expect("memory store");
    glass
        .publish_post(PublishPost {
            session_id: None,
            session_title: Some("report lane".to_string()),
            agent: Some("report-agent".to_string()),
            title: "Reportable post".to_string(),
            surfaces: vec![
                Surface::new(
                    SurfaceKind::Metric,
                    json!({"label": "status", "value": "green", "feedKind": "blocked"}),
                )
                .unwrap(),
            ],
        })
        .expect("seed post");
    let app = app_router(glass);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "kind": "activity-digest",
                        "scope": { "type": "fleet" },
                        "window": "today",
                        "requestedBy": "test"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["url"], "/reports/R-001");
    assert_eq!(value["cached"], false);
    assert!(value["html"].as_str().unwrap().contains("Reportable post"));

    let reopened = app
        .oneshot(
            Request::builder()
                .uri("/reports/R-001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("reopen response");
    assert_eq!(reopened.status(), StatusCode::OK);
    let body = reopened.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert_shared_rail(&html, Some("/reports"));
    assert!(html.contains("Activity digest - fleet"));
    assert!(html.contains("Reportable post"));
    assert!(html.contains("blocked"));
}

#[tokio::test]
async fn custom_activity_report_generation_uses_cache_until_regenerated() {
    let glass = Glass::memory().expect("memory store");
    glass
        .publish_post(PublishPost {
            session_id: None,
            session_title: Some("custom report lane".to_string()),
            agent: Some("range-agent".to_string()),
            title: "Custom range post".to_string(),
            surfaces: vec![
                Surface::new(
                    SurfaceKind::Markdown,
                    json!({"markdown": "inside the chosen range", "feedSummary": "custom range"}),
                )
                .unwrap(),
            ],
        })
        .expect("seed post");
    let app = app_router(glass);
    let today = Local::now().date_naive();
    let tomorrow = today + ChronoDuration::days(1);
    let payload = json!({
        "kind": "activity-digest",
        "scope": "fleet",
        "window": {
            "type": "custom",
            "start": today.format("%Y-%m-%d").to_string(),
            "end": tomorrow.format("%Y-%m-%d").to_string()
        },
        "requestedBy": "test"
    });

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("first response");
    assert_eq!(first.status(), StatusCode::OK);
    let body = first.into_body().collect().await.unwrap().to_bytes();
    let first_value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(first_value["id"], "R-001");
    assert_eq!(first_value["cached"], false);

    let repeat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("repeat response");
    assert_eq!(repeat.status(), StatusCode::OK);
    let body = repeat.into_body().collect().await.unwrap().to_bytes();
    let repeat_value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(repeat_value["id"], "R-001");
    assert_eq!(repeat_value["url"], "/reports/R-001");
    assert_eq!(repeat_value["cached"], true);
    assert!(
        repeat_value["cacheNote"]
            .as_str()
            .unwrap()
            .starts_with("cached")
    );

    let mut regenerate_payload = payload.clone();
    regenerate_payload["regenerate"] = json!(true);
    let regenerated = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports")
                .header("content-type", "application/json")
                .body(Body::from(regenerate_payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("regenerate response");
    assert_eq!(regenerated.status(), StatusCode::OK);
    let body = regenerated.into_body().collect().await.unwrap().to_bytes();
    let regenerated_value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(regenerated_value["id"], "R-002");
    assert_eq!(regenerated_value["url"], "/reports/R-002");
    assert_eq!(regenerated_value["cached"], false);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/reports")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("cache listing response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let reports = value["reports"].as_array().expect("reports array");
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0]["id"], "R-002");
    assert_eq!(reports[1]["id"], "R-001");
    assert_eq!(reports[0]["meta"]["window"]["preset"], "custom");
}

#[tokio::test]
async fn review_index_report_generation_persists_the_review_document() {
    let app = app_router(Glass::memory().expect("memory store"));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "kind": "review-index",
                        "scope": "fleet",
                        "requestedBy": "test"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["url"], "/reports/R-001");

    let reopened = app
        .oneshot(
            Request::builder()
                .uri("/reports/R-001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("reopen response");
    assert_eq!(reopened.status(), StatusCode::OK);
    let body = reopened.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Review index"));
    assert!(html.contains("Persisted review surfaces"));
    assert!(html.contains("Review surface sample diff"));
}

#[tokio::test]
async fn backlog_report_streams_a_skeleton_then_a_named_config_error_when_powder_unconfigured() {
    // No GLASS_POWDER_API_BASE_URL/GLASS_POWDER_API_KEY configured in the
    // test process: the fetch deterministically fails fast, exercising the
    // real skeleton-then-error contract without live Powder access.
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/api/backlog/glass")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("event: skeleton"));
    assert!(
        text.contains("event: error") && text.contains("GLASS_POWDER_API_BASE_URL"),
        "an unconfigured Powder connection must fail loudly with the missing env var \
         named, not silently: {text}"
    );
}

#[tokio::test]
async fn review_sample_shell_renders_the_three_cited_context_layers() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/review/sample")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert_shared_rail(&html, None);
    assert!(html.contains("Narrated review"));
    assert!(html.contains("Change context"));
    assert!(html.contains("src/lib.rs:777"));
    assert!(html.contains("Powder ticket"));
    assert!(html.contains("glass-902"));
    assert!(html.contains("VISION.md#live-stage"));
    assert!(html.contains(r#"data-glance-component="disclosure""#));
    assert!(html.contains("Raw diff"));
    assert!(
        html.contains(r#"data-reviewer-sanity="pass""#),
        "the operator-facing sample must show the reviewer sanity check passed"
    );
}

#[tokio::test]
async fn needs_you_shell_serves_the_rail_page() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/needs-you")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert_shared_rail(&html, Some("/needs-you"));
    assert!(html.contains("Needs You"));
    assert!(
        html.contains("ny-dialog"),
        "the draft-safety sheet dialog must be present"
    );
}

#[tokio::test]
async fn needs_you_report_streams_a_skeleton_then_a_named_config_error_when_bb_unconfigured() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .uri("/api/needs-you")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("event: skeleton"));
    assert!(
        text.contains("event: error") && text.contains("GLASS_BITTERBLOSSOM_API_BASE_URL"),
        "an unconfigured Bitterblossom connection must fail loudly, not silently: {text}"
    );
}

#[tokio::test]
async fn needs_you_answer_rejects_an_empty_answer_before_reaching_bb() {
    let response = app_router(Glass::memory().expect("memory store"))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/needs-you/answer")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"ask_id": "ask-1", "answer": "   "}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nonce}.db"))
}

fn landmark_env_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

struct EnvVarGuard {
    key: &'static str,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        // SAFETY: this test holds the process-wide landmark_env_lock before
        // mutating this process env var, and the guard removes the var before
        // the lock is released.
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // SAFETY: see EnvVarGuard::set; this is the paired cleanup while the
        // same async test still owns the process-wide env lock.
        unsafe {
            std::env::remove_var(self.key);
        }
    }
}
