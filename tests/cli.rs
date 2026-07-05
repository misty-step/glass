use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::Value;

fn glass_bin() -> &'static str {
    env!("CARGO_BIN_EXE_glass")
}

fn temp_db(name: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("glass-cli-test-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn publish_wraps_the_same_core_the_mcp_tool_calls() {
    let db = temp_db("publish");

    let output = Command::new(glass_bin())
        .args([
            "publish",
            "--db",
            db.to_str().unwrap(),
            "--title",
            "cli publish test",
            "--agent",
            "cli-test",
            "--markdown",
            "hello from the cli",
            "--json",
        ])
        .output()
        .expect("run glass publish");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: Value = serde_json::from_slice(&output.stdout).expect("parse publish JSON");
    assert_eq!(outcome["post"]["title"], "cli publish test");
    assert_eq!(outcome["post"]["surfaces"][0]["kind"], "markdown");
    assert_eq!(
        outcome["post"]["surfaces"][0]["markdown"],
        "hello from the cli"
    );
    assert!(outcome["url"].as_str().unwrap().starts_with("/session/"));

    let _ = std::fs::remove_file(&db);
}

#[test]
fn publish_without_title_fails_loudly() {
    let db = temp_db("publish-no-title");

    let output = Command::new(glass_bin())
        .args([
            "publish",
            "--db",
            db.to_str().unwrap(),
            "--markdown",
            "no title here",
        ])
        .output()
        .expect("run glass publish");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--title is required"));

    let _ = std::fs::remove_file(&db);
}

#[test]
fn publish_without_any_surface_fails_loudly() {
    let db = temp_db("publish-no-surface");

    let output = Command::new(glass_bin())
        .args([
            "publish",
            "--db",
            db.to_str().unwrap(),
            "--title",
            "no surfaces here",
        ])
        .output()
        .expect("run glass publish");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("at least one surface is required"));

    let _ = std::fs::remove_file(&db);
}

#[test]
fn publish_reads_surfaces_json_from_stdin() {
    let db = temp_db("publish-stdin");

    let mut child = Command::new(glass_bin())
        .args([
            "publish",
            "--db",
            db.to_str().unwrap(),
            "--title",
            "stdin surfaces",
            "--surfaces-json",
            "-",
            "--json",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn glass publish");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"[{"kind":"terminal","text":"$ ok"}]"#)
        .expect("write stdin");

    let output = child.wait_with_output().expect("wait for glass publish");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: Value = serde_json::from_slice(&output.stdout).expect("parse publish JSON");
    assert_eq!(outcome["post"]["surfaces"][0]["kind"], "terminal");

    let _ = std::fs::remove_file(&db);
}

#[test]
fn feedback_drains_real_user_comments_exactly_once() {
    let db = temp_db("feedback");

    // publish a post to create a session, capturing its id.
    let publish_output = Command::new(glass_bin())
        .args([
            "publish",
            "--db",
            db.to_str().unwrap(),
            "--title",
            "feedback test post",
            "--markdown",
            "waiting for feedback",
            "--json",
        ])
        .output()
        .expect("run glass publish");
    assert!(publish_output.status.success());
    let outcome: Value = serde_json::from_slice(&publish_output.stdout).expect("parse publish");
    let session_id = outcome["post"]["session_id"].as_str().unwrap().to_owned();
    let post_id = outcome["post"]["id"].as_str().unwrap().to_owned();

    // no feedback yet.
    let empty = Command::new(glass_bin())
        .args([
            "feedback",
            "--db",
            db.to_str().unwrap(),
            "--session",
            &session_id,
        ])
        .output()
        .expect("run glass feedback");
    assert!(empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stdout).contains("no new comments"));

    // insert real user feedback directly through the same core the CLI wraps,
    // using a second publish call's session to reuse glass's own comment path
    // is awkward from the CLI alone (no comment subcommand exists), so drive
    // the HTTP API the same way a real user's browser would.
    let bind = "127.0.0.1:9241";
    let mut server = Command::new(glass_bin())
        .args(["serve", "--db", db.to_str().unwrap(), "--bind", bind])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn glass serve");
    std::thread::sleep(std::time::Duration::from_millis(400));

    let body = serde_json::json!({
        "session_id": session_id,
        "post_id": post_id,
        "author": "user",
        "text": "ship it",
    })
    .to_string();
    let curl = Command::new("curl")
        .args([
            "-fsS",
            "-X",
            "POST",
            &format!("http://{bind}/api/comments"),
            "-H",
            "Content-Type: application/json",
            "-d",
            &body,
        ])
        .output()
        .expect("post comment via curl");
    assert!(
        curl.status.success(),
        "curl stderr: {}",
        String::from_utf8_lossy(&curl.stderr)
    );
    let _ = server.kill();
    let _ = server.wait();

    // now the CLI should drain exactly this one comment.
    let drained = Command::new(glass_bin())
        .args([
            "feedback",
            "--db",
            db.to_str().unwrap(),
            "--session",
            &session_id,
            "--json",
        ])
        .output()
        .expect("run glass feedback");
    assert!(drained.status.success());
    let comments: Value = serde_json::from_slice(&drained.stdout).expect("parse feedback JSON");
    assert_eq!(comments.as_array().unwrap().len(), 1);
    assert_eq!(comments[0]["text"], "ship it");
    assert_eq!(comments[0]["author"], "user");

    // exactly-once: draining again returns nothing new.
    let drained_again = Command::new(glass_bin())
        .args([
            "feedback",
            "--db",
            db.to_str().unwrap(),
            "--session",
            &session_id,
        ])
        .output()
        .expect("run glass feedback");
    assert!(drained_again.status.success());
    assert!(String::from_utf8_lossy(&drained_again.stdout).contains("no new comments"));

    let _ = std::fs::remove_file(&db);
}

#[test]
fn feedback_without_session_fails_loudly() {
    let db = temp_db("feedback-no-session");

    let output = Command::new(glass_bin())
        .args(["feedback", "--db", db.to_str().unwrap()])
        .output()
        .expect("run glass feedback");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--session is required"));

    let _ = std::fs::remove_file(&db);
}
