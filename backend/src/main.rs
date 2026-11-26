use neondojo::{
    api::{self, AppState},
    engine::Engine,
    jobs,
};
use tracing_subscriber::EnvFilter;

mod database_pool;

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
    let pool = database_pool::options()
        .max_connections(database_pool::max_connections(
            std::env::var("NEONDOJO_DB_MAX_CONNECTIONS").ok().as_deref(),
        )?)
        .acquire_timeout(database_pool::ACQUIRE_TIMEOUT)
        .connect(&url)
        .await?;
    if let Ok(expected) = std::env::var("NEONDOJO_E2E_DATABASE_NAME") {
        let actual: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await?;
        anyhow::ensure!(
            expected.starts_with("neondojo_e2e_") && actual == expected,
            "Browser test backend must use the provisioned test database"
        );
    }
    neondojo::schema_migrations::run(&pool).await?;
    neondojo::benchmarks::recover_interrupted(&pool).await?;
    while neondojo::databases::backfill_source_metadata(&pool).await? > 0 {}
    while neondojo::play::backfill_library_matches(&pool).await? > 0 {}
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
    let browse_worker = tokio::spawn(neondojo::browse_indexes::worker(state.clone()));
    let assessment_worker = tokio::spawn(neondojo::match_assessment::worker(state.clone()));
    let match_worker = tokio::spawn(neondojo::engine_matches::worker(state.clone()));
    let anki_worker = tokio::spawn(neondojo::learning::anki_sync_worker(state.clone()));
    let web_dir = std::env::var_os("NEONDOJO_WEB_DIR").map(std::path::PathBuf::from);
    let app = api::router_with_web_dir(state, web_dir.as_deref());
    let address = std::env::var("NEONDOJO_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    let address = listener.local_addr()?;
    tracing::info!(%address,"NeonDojo listening");
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
    match_worker.await?;
    assessment_worker.await?;
    browse_worker.await?;
    anki_worker.await?;
    Ok(())
}
