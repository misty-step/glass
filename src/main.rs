use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use glass::{
    DoctorConfig, Glass, PublishPost, SURFACE_KINDS, Surface, SurfaceKind, app_router, run_doctor,
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None | Some("serve") => serve(args.collect()).await,
        Some("surface-kinds") => {
            println!("{}", serde_json::to_string_pretty(&SURFACE_KINDS)?);
            Ok(())
        }
        Some("doctor") => doctor(args.collect()).await,
        Some("publish") => publish(args.collect()).await,
        Some("help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        Some(command) => {
            print_help();
            bail!("unknown command: {command}")
        }
    }
}

async fn doctor(args: Vec<String>) -> Result<()> {
    let mut url = std::env::var("GLASS_URL").unwrap_or_else(|_| "http://127.0.0.1:9041".into());
    let mut db = std::env::var("GLASS_DB").unwrap_or_else(|_| "data/glass.db".into());
    let mut timeout_seconds = 5_u64;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--url" => url = iter.next().context("--url requires a URL")?,
            "--db" => db = iter.next().context("--db requires a path")?,
            "--timeout" => {
                timeout_seconds = iter
                    .next()
                    .context("--timeout requires seconds")?
                    .parse()
                    .context("parse --timeout seconds")?;
            }
            other => bail!("unknown doctor argument: {other}"),
        }
    }

    let report = run_doctor(DoctorConfig {
        url,
        db_path: PathBuf::from(db),
        timeout: Duration::from_secs(timeout_seconds),
    })
    .await?;
    println!(
        "glass doctor ok\nurl={}\ndb={}\nsessions={}\nprobe_session={}\nprobe_post={}\nprobe=self-cleaned",
        report.url,
        report.db_path.display(),
        report.session_count,
        report.probe_session_id,
        report.probe_post_id
    );
    Ok(())
}

async fn serve(args: Vec<String>) -> Result<()> {
    let mut bind = std::env::var("GLASS_BIND").unwrap_or_else(|_| "127.0.0.1:9041".into());
    let mut db = std::env::var("GLASS_DB").unwrap_or_else(|_| "data/glass.db".into());
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--bind" => bind = iter.next().context("--bind requires an address")?,
            "--db" => db = iter.next().context("--db requires a path")?,
            other => bail!("unknown serve argument: {other}"),
        }
    }
    let addr: SocketAddr = bind
        .parse()
        .with_context(|| format!("parse bind address {bind}"))?;
    let glass = Glass::open(&db)?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    println!("glass listening on http://{addr} with db {db}");
    axum::serve(listener, app_router(glass)).await?;
    Ok(())
}

async fn publish(args: Vec<String>) -> Result<()> {
    let mut db = std::env::var("GLASS_DB").unwrap_or_else(|_| "data/glass.db".into());
    let mut title: Option<String> = None;
    let mut session_id: Option<String> = None;
    let mut session_title: Option<String> = None;
    let mut agent: Option<String> = None;
    let mut surfaces: Vec<Surface> = Vec::new();
    let mut json_output = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--db" => db = iter.next().context("--db requires a path")?,
            "--title" => title = Some(iter.next().context("--title requires text")?),
            "--session" => session_id = Some(iter.next().context("--session requires an id")?),
            "--session-title" => {
                session_title = Some(iter.next().context("--session-title requires text")?);
            }
            "--agent" => agent = Some(iter.next().context("--agent requires a name")?),
            "--markdown" => {
                let text = iter.next().context("--markdown requires text")?;
                surfaces.push(Surface::new(
                    SurfaceKind::Markdown,
                    json!({ "markdown": text }),
                )?);
            }
            "--markdown-file" => {
                let path = iter.next().context("--markdown-file requires a path")?;
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("read --markdown-file {path}"))?;
                surfaces.push(Surface::new(
                    SurfaceKind::Markdown,
                    json!({ "markdown": text }),
                )?);
            }
            "--terminal" => {
                let text = iter.next().context("--terminal requires text")?;
                surfaces.push(Surface::new(
                    SurfaceKind::Terminal,
                    json!({ "text": text }),
                )?);
            }
            "--terminal-file" => {
                let path = iter.next().context("--terminal-file requires a path")?;
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("read --terminal-file {path}"))?;
                surfaces.push(Surface::new(
                    SurfaceKind::Terminal,
                    json!({ "text": text }),
                )?);
            }
            "--surfaces-json" => {
                let path = iter
                    .next()
                    .context("--surfaces-json requires a path (or -)")?;
                let raw = if path == "-" {
                    let mut buf = String::new();
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
                    buf
                } else {
                    std::fs::read_to_string(&path)
                        .with_context(|| format!("read --surfaces-json {path}"))?
                };
                let extra: Vec<Surface> =
                    serde_json::from_str(&raw).context("parse --surfaces-json")?;
                surfaces.extend(extra);
            }
            "--json" => json_output = true,
            other => bail!("unknown publish argument: {other}"),
        }
    }

    let title = title.context("--title is required")?;
    if surfaces.is_empty() {
        bail!(
            "at least one surface is required: --markdown, --markdown-file, --terminal, --terminal-file, or --surfaces-json"
        );
    }

    let glass = Glass::open(&db)?;
    let outcome = glass.publish_post(PublishPost {
        session_id,
        session_title,
        agent,
        title,
        surfaces,
    })?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        println!(
            "glass publish ok\npost_id={}\nsession_id={}\nurl={}",
            outcome.post.id, outcome.post.session_id, outcome.url
        );
    }
    Ok(())
}

fn print_help() {
    eprintln!(
        "glass commands:\n  glass serve [--bind 127.0.0.1:9041] [--db data/glass.db]\n  glass doctor [--url http://127.0.0.1:9041] [--db data/glass.db] [--timeout 5]\n  glass surface-kinds\n  glass publish --title <title> [--db data/glass.db] [--session <id>] [--session-title <title>] [--agent <name>] [--markdown <text>] [--markdown-file <path>] [--terminal <text>] [--terminal-file <path>] [--surfaces-json <path>|-] [--json]"
    );
}
