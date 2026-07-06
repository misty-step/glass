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
fn publish_outcome_carries_no_feedback_surface() {
    let db = temp_db("one-way");

    let output = Command::new(glass_bin())
        .args([
            "publish",
            "--db",
            db.to_str().unwrap(),
            "--title",
            "one-way test post",
            "--markdown",
            "glass is one-way",
            "--json",
        ])
        .output()
        .expect("run glass publish");

    assert!(output.status.success());
    let outcome: Value = serde_json::from_slice(&output.stdout).expect("parse publish JSON");
    assert!(
        outcome.get("userFeedback").is_none() && outcome.get("user_feedback").is_none(),
        "publish outcome must not carry a feedback field: {outcome}"
    );

    let _ = std::fs::remove_file(&db);
}
