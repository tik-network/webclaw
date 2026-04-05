mod auth;
mod cloud;
mod error;
mod jobs;
mod routes;
mod state;

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "webclaw-api", about = "Self-hosted webclaw REST API server")]
struct Args {
    /// Listen port
    #[arg(long, default_value = "3000", env = "WEBCLAW_API_PORT")]
    port: u16,

    /// Listen host
    #[arg(long, default_value = "0.0.0.0", env = "WEBCLAW_API_HOST")]
    host: String,

    /// API key for bearer auth (optional — if not set, all requests are accepted)
    #[arg(long, env = "WEBCLAW_API_KEY")]
    api_key: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    verbose: bool,
}

/// Load config from `~/.webclaw/config.json`.
fn load_config() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let path = PathBuf::from(home).join(".webclaw").join("config.json");
    let Ok(data) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) else {
        eprintln!("warning: failed to parse {}", path.display());
        return;
    };
    let Some(obj) = json.as_object() else {
        return;
    };
    for (key, value) in obj {
        let env_key = key.to_uppercase();
        if std::env::var_os(&env_key).is_none() {
            if let Some(v) = value.as_str() {
                // SAFETY: called once at startup before any threads are spawned.
                unsafe { std::env::set_var(&env_key, v) };
            }
        }
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    load_config();

    let args = Args::parse();

    let filter = if args.verbose {
        "debug,hyper=info,h2=info,rustls=info,boring=info"
    } else {
        "info,hyper=warn,h2=warn,rustls=warn,boring=warn"
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)))
        .init();

    let state = Arc::new(state::AppState::new(args.api_key).await);
    let app = routes::router(state);

    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");

    info!("webclaw-api v{} listening on {addr}", env!("CARGO_PKG_VERSION"));
    if args.verbose {
        info!("debug logging enabled");
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install signal handler");
    info!("shutting down...");
}
