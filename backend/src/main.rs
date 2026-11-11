use bgtutor::{
    api::{self, AppState},
    engine::Engine,
    jobs,
};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Only this project's .env: do not accidentally load a parent project's database.
    dotenvy::from_path(".env").ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "bgtutor=info,tower_http=info".into()),
        )
        .init();
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://bgtutor:bgtutor_local@127.0.0.1:54329/bgtutor".into());
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&url)
        .await?;
    if let Ok(expected) = std::env::var("BGTUTOR_E2E_DATABASE_NAME") {
        let actual: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await?;
        anyhow::ensure!(
            expected.starts_with("bgtutor_e2e_") && actual == expected,
            "Browser test backend must use the provisioned test database"
        );
    }
    sqlx::migrate!().run(&pool).await?;
    bgtutor::benchmarks::recover_interrupted(&pool).await?;
    while bgtutor::databases::backfill_source_metadata(&pool).await? > 0 {}
    while bgtutor::play::backfill_library_matches(&pool).await? > 0 {}
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    match engine.version().await {
        Ok(version) => tracing::info!(%version,"Local engine ready"),
        Err(error) => {
            tracing::warn!(%error,"GNUbg unavailable; analysis jobs will fail until configured")
        }
    }
    let state = AppState::new(pool, engine);
    let stop = state.shutdown.clone();
    let worker = tokio::spawn(jobs::worker(state.clone(), stop.clone()));
    let mut app = api::router(state);
    if let Ok(dir) = std::env::var("BGTUTOR_WEB_DIR") {
        app = app.fallback_service(tower_http::services::ServeDir::new(&dir).not_found_service(
            tower_http::services::ServeFile::new(format!("{dir}/index.html")),
        ));
    }
    let address = std::env::var("BGTUTOR_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    let address = listener.local_addr()?;
    tracing::info!(%address,"BG Tutor listening");
    let shutdown = stop.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            #[cfg(unix)]
            {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("signal handler");
                tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=terminate.recv()=>{} }
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
            }
            shutdown.cancel();
        })
        .await?;
    stop.cancel();
    worker.await?;
    Ok(())
}
