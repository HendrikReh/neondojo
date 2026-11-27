//! Start and stop a NeonDojo runtime (headless core or HTTP server) without
//! installing process-wide handlers.

use crate::{api, database_pool, engine::Engine, facade::BackendFacade};
use anyhow::{Context, Result};
use sqlx::postgres::PgConnectOptions;
use std::{future::Future, net::SocketAddr, path::PathBuf};
use tokio::{
    net::TcpListener,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

/// Configuration for the transport-agnostic NeonDojo core runtime (database
/// pool, migrations, backfills, engines, and background workers).
pub struct CoreConfig {
    pub database: PgConnectOptions,
    pub gnubg_binary: String,
    /// Optional safety pin for an isolated test database.
    pub expected_test_database: Option<String>,
    max_connections: u32,
}

impl CoreConfig {
    pub fn new(database: PgConnectOptions) -> Self {
        Self {
            database,
            gnubg_binary: "gnubg".into(),
            expected_test_database: None,
            max_connections: database_pool::DEFAULT_MAX_CONNECTIONS,
        }
    }

    pub fn with_max_connections(mut self, configured: &str) -> Result<Self> {
        self.max_connections = database_pool::max_connections(Some(configured))?;
        Ok(self)
    }
}

/// Owns the initialized [`api::AppState`] and all five background workers
/// without binding an HTTP socket. Dropping it requests shutdown; call
/// [`Self::shutdown`] to drain workers, engine sidecars, and the pool cleanly.
pub struct CoreRuntime {
    state: api::AppState,
    stop: CancellationToken,
    task: JoinHandle<Result<()>>,
}

impl CoreRuntime {
    pub async fn start(config: CoreConfig) -> Result<Self> {
        let state = init_state(config).await?;
        let stop = state.shutdown.clone();
        let workers = spawn_workers(&state, &stop);
        let task = tokio::spawn(supervise_core(state.clone(), workers));
        Ok(Self { state, stop, task })
    }

    pub fn state(&self) -> &api::AppState {
        &self.state
    }

    pub fn facade(&self) -> BackendFacade {
        BackendFacade::new(self.state.clone())
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.stop.clone()
    }

    pub async fn shutdown(mut self) -> Result<()> {
        self.stop.cancel();
        (&mut self.task).await?
    }
}

impl Drop for CoreRuntime {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

pub struct RuntimeConfig {
    pub database: PgConnectOptions,
    pub bind: String,
    pub gnubg_binary: String,
    pub web_dir: Option<PathBuf>,
    /// Optional safety pin for an isolated browser-test database.
    pub expected_test_database: Option<String>,
    max_connections: u32,
}

impl RuntimeConfig {
    pub fn new(database: PgConnectOptions) -> Self {
        Self {
            database,
            bind: "127.0.0.1:8787".into(),
            gnubg_binary: "gnubg".into(),
            web_dir: None,
            expected_test_database: None,
            max_connections: database_pool::DEFAULT_MAX_CONNECTIONS,
        }
    }

    pub fn with_max_connections(mut self, configured: &str) -> Result<Self> {
        self.max_connections = database_pool::max_connections(Some(configured))?;
        Ok(self)
    }

    fn into_core_and_http(self) -> (CoreConfig, String, Option<PathBuf>) {
        let core = CoreConfig {
            database: self.database,
            gnubg_binary: self.gnubg_binary,
            expected_test_database: self.expected_test_database,
            max_connections: self.max_connections,
        };
        (core, self.bind, self.web_dir)
    }
}

/// Owns the listener and workers. Dropping it requests shutdown. Use
/// [Self::shutdown] to also wait for active requests and workers to drain.
pub struct RunningServer {
    address: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<Result<()>>,
}

impl RunningServer {
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    pub async fn shutdown(self) -> Result<()> {
        self.stop.cancel();
        self.run_until(std::future::pending::<()>()).await
    }

    /// Wait for the runtime to exit or for the caller to request shutdown.
    pub async fn run_until(mut self, signal: impl Future<Output = ()>) -> Result<()> {
        tokio::select! {
            result = &mut self.task => result?,
            () = signal => {
                self.stop.cancel();
                (&mut self.task).await?
            }
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

async fn init_state(config: CoreConfig) -> Result<api::AppState> {
    let pool = database_pool::options()
        .max_connections(config.max_connections)
        .acquire_timeout(database_pool::ACQUIRE_TIMEOUT)
        .connect_with(config.database)
        .await?;
    if let Some(expected) = config.expected_test_database {
        let actual: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await?;
        anyhow::ensure!(
            expected.starts_with("neondojo_e2e_") && actual == expected,
            "Browser test backend must use the provisioned test database"
        );
    }
    crate::schema_migrations::run(&pool).await?;
    crate::benchmarks::recover_interrupted(&pool).await?;
    while crate::databases::backfill_source_metadata(&pool).await? > 0 {}
    while crate::play::backfill_library_matches(&pool).await? > 0 {}
    let engine = Engine::new(config.gnubg_binary);
    match engine.version().await {
        Ok(version) => tracing::info!(%version, "Local engine ready"),
        Err(error) => {
            tracing::warn!(%error, "GNUbg unavailable; analysis jobs will fail until configured")
        }
    }
    Ok(api::AppState::new(pool, engine))
}

fn spawn_workers(state: &api::AppState, stop: &CancellationToken) -> JoinSet<()> {
    let mut workers = JoinSet::new();
    workers.spawn(crate::jobs::worker(state.clone(), stop.clone()));
    workers.spawn(crate::browse_indexes::worker(state.clone()));
    workers.spawn(crate::match_assessment::worker(state.clone()));
    workers.spawn(crate::engine_matches::worker(state.clone()));
    workers.spawn(crate::learning::anki_sync_worker(state.clone()));
    workers
}

async fn drain_runtime(
    state: &api::AppState,
    mut workers: JoinSet<()>,
    mut result: Result<()>,
) -> Result<()> {
    state.shutdown.cancel();
    while let Some(worker) = workers.join_next().await {
        result = result.and(worker.context("NeonDojo worker failed"));
    }
    state.background_tasks.close();
    state.background_tasks.wait().await;
    crate::engines::shutdown_workers().await;
    state.pool.close().await;
    result
}

async fn supervise_core(state: api::AppState, mut workers: JoinSet<()>) -> Result<()> {
    let stop = state.shutdown.clone();
    let result = tokio::select! {
        () = stop.cancelled() => Ok(()),
        worker = workers.join_next() => match worker {
            Some(Err(error)) => Err(error).context("NeonDojo worker failed"),
            _ if stop.is_cancelled() => Ok(()),
            _ => Err(anyhow::anyhow!("NeonDojo worker exited before shutdown")),
        },
    };
    drain_runtime(&state, workers, result).await
}

pub async fn start(config: RuntimeConfig) -> Result<RunningServer> {
    let (core_config, bind, web_dir) = config.into_core_and_http();
    let listener = TcpListener::bind(&bind)
        .await
        .with_context(|| format!("Bind NeonDojo to {bind}"))?;
    let address = listener.local_addr()?;
    let state = init_state(core_config).await?;
    let stop = state.shutdown.clone();
    let task = tokio::spawn(serve(listener, state, web_dir));
    Ok(RunningServer {
        address,
        stop,
        task,
    })
}

async fn serve(
    listener: TcpListener,
    state: api::AppState,
    web_dir: Option<PathBuf>,
) -> Result<()> {
    let stop = state.shutdown.clone();
    let mut workers = spawn_workers(&state, &stop);
    let app = api::router_with_web_dir(state.clone(), web_dir.as_deref());
    let server = axum::serve(listener, app).with_graceful_shutdown(stop.clone().cancelled_owned());
    let mut server = std::pin::pin!(std::future::IntoFuture::into_future(server));
    let result = tokio::select! {
        result = &mut server => result.context("Serve NeonDojo"),
        worker = workers.join_next() => {
            let result = match worker {
                Some(Err(error)) => Err(error).context("NeonDojo worker failed"),
                _ if stop.is_cancelled() => Ok(()),
                _ => Err(anyhow::anyhow!("NeonDojo worker exited before shutdown")),
            };
            stop.cancel();
            result.and(server.await.context("Serve NeonDojo"))
        }
    };
    drain_runtime(&state, workers, result).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[sqlx::test(migrations = false)]
    async fn core_runtime_starts_and_stops_twice_without_http(pool: sqlx::PgPool) {
        let dir = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            let mut config = CoreConfig::new((*pool.connect_options()).clone());
            config.gnubg_binary = dir.path().join("missing-gnubg").display().to_string();
            let runtime = CoreRuntime::start(config).await.unwrap();
            let health = runtime.facade().health().await.unwrap();
            assert!(health.database);
            assert!(health.gnubg.is_none());

            tokio::time::timeout(std::time::Duration::from_secs(5), runtime.shutdown())
                .await
                .expect("all background workers should stop")
                .unwrap();
        }
    }

    #[sqlx::test(migrations = false)]
    async fn starts_and_stops_twice_against_the_same_store(pool: sqlx::PgPool) {
        let web = tempfile::tempdir().unwrap();
        std::fs::write(web.path().join("index.html"), "embedded NeonDojo").unwrap();
        let mut bind = "127.0.0.1:0".to_owned();
        for _ in 0..2 {
            let mut config = RuntimeConfig::new((*pool.connect_options()).clone());
            config.bind = bind;
            config.web_dir = Some(web.path().to_path_buf());
            config.gnubg_binary = web.path().join("missing-gnubg").display().to_string();
            let server = start(config).await.unwrap();
            let address = server.local_addr();
            let mut connection = tokio::net::TcpStream::connect(address).await.unwrap();
            connection
                .write_all(
                    format!("GET / HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                )
                .await
                .unwrap();
            let mut response = String::new();
            connection.read_to_string(&mut response).await.unwrap();
            assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
            assert!(response.ends_with("embedded NeonDojo"), "{response}");

            tokio::time::timeout(std::time::Duration::from_secs(5), server.shutdown())
                .await
                .expect("all background workers should stop")
                .unwrap();
            assert!(tokio::net::TcpStream::connect(address).await.is_err());
            bind = address.to_string();
        }
    }
}
