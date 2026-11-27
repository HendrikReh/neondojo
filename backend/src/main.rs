use neondojo::runtime::{self, RuntimeConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Only this project's .env: do not accidentally load a parent project's database.
    dotenvy::from_path(".env").ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "neondojo=info,tower_http=info".into()),
        )
        .init();

    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://neondojo:neondojo_local@127.0.0.1:54329/neondojo".into());
    let mut config = RuntimeConfig::new(url.parse()?);
    if let Ok(max_connections) = std::env::var("NEONDOJO_DB_MAX_CONNECTIONS") {
        config = config.with_max_connections(&max_connections)?;
    }
    if let Ok(bind) = std::env::var("NEONDOJO_BIND") {
        config.bind = bind;
    }
    if let Ok(binary) = std::env::var("GNUBG_BIN") {
        config.gnubg_binary = binary;
    }
    config.web_dir = std::env::var_os("NEONDOJO_WEB_DIR").map(std::path::PathBuf::from);
    config.expected_test_database = std::env::var("NEONDOJO_E2E_DATABASE_NAME").ok();

    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let server = runtime::start(config).await?;
    tracing::info!(address = %server.local_addr(), "NeonDojo listening");
    server
        .run_until(async {
            #[cfg(unix)]
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}
