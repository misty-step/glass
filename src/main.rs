use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use glass::{DoctorConfig, Glass, SURFACE_KINDS, app_router, run_doctor};

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
        "glass doctor ok\nurl={}\ndb={}\nsessions={}\nprobe_session={}\nprobe_post={}\nfeedback=delivered-once\nprobe=self-cleaned",
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

fn print_help() {
    eprintln!(
        "glass commands:\n  glass serve [--bind 127.0.0.1:9041] [--db data/glass.db]\n  glass doctor [--url http://127.0.0.1:9041] [--db data/glass.db] [--timeout 5]\n  glass surface-kinds"
    );
}
