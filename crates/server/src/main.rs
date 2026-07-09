//! `asmteacher` — serve the API and the built frontend.

use clap::Parser;
use server::{app, AppState};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "asmteacher", about = "The Assembly Teacher server", version)]
struct Args {
    /// Address to listen on. TLS is expected to be terminated upstream.
    #[arg(long, env = "ASMTEACHER_LISTEN", default_value = "127.0.0.1:8080")]
    listen: SocketAddr,

    /// Directory of built frontend assets. Omit to serve the API only.
    #[arg(long, env = "ASMTEACHER_WEB")]
    web: Option<PathBuf>,

    /// Directory containing the curriculum.
    #[arg(long, env = "ASMTEACHER_LESSONS", default_value = "lessons")]
    lessons: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=warn".into()),
        )
        .init();

    let args = Args::parse();

    let curriculum = lesson::load(&args.lessons)
        .map_err(|e| format!("could not load lessons from {}: {e}", args.lessons.display()))?;

    // Refuse to serve a curriculum that does not validate. A lesson whose
    // reference answer is wrong is worse than a missing lesson.
    let issues = lesson::validate(&curriculum);
    if !issues.is_empty() {
        for issue in &issues {
            tracing::error!("{issue}");
        }
        return Err(format!("{} lesson(s) failed validation", issues.len()).into());
    }

    tracing::info!(lessons = curriculum.len(), parts = curriculum.parts.len(), "curriculum loaded");

    if let Some(web) = &args.web {
        if !web.is_dir() {
            return Err(
                format!("{} is not a directory; run contrib/build.sh", web.display()).into()
            );
        }
    }

    let state = Arc::new(AppState {
        // Loaded once, immutable for the life of the process.
        curriculum: Box::leak(Box::new(curriculum)),
        web_dir: args.web,
    });

    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!("listening on http://{}", listener.local_addr()?);

    axum::serve(listener, app(state)).with_graceful_shutdown(shutdown()).await?;
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
