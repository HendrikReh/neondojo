//! Start and stop a NeonDojo server without installing process-wide handlers.

use crate::{api, database_pool, engine::Engine};
use anyhow::{Context, Result};
use sqlx::postgres::PgConnectOptions;
use std::{future::Future, net::SocketAddr, path::PathBuf};
use tokio::{
    net::TcpListener,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

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

pub async fn start(config: RuntimeConfig) -> Result<RunningServer> {
    let listener = TcpListener::bind(&config.bind)
        .await
        .with_context(|| format!("Bind NeonDojo to {}", config.bind))?;
    let address = listener.local_addr()?;
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

    let state = api::AppState::new(pool, engine);
    let stop = state.shutdown.clone();
    let task = tokio::spawn(serve(listener, state, config.web_dir));
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
    let pool = state.pool.clone();
    let mut workers = JoinSet::new();
    workers.spawn(crate::jobs::worker(state.clone(), stop.clone()));
    workers.spawn(crate::browse_indexes::worker(state.clone()));
    workers.spawn(crate::match_assessment::worker(state.clone()));
    workers.spawn(crate::engine_matches::worker(state.clone()));
    workers.spawn(crate::learning::anki_sync_worker(state.clone()));
    let app = api::router_with_web_dir(state, web_dir.as_deref());
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
    stop.cancel();
    let mut result = result;
    while let Some(worker) = workers.join_next().await {
        result = result.and(worker.context("NeonDojo worker failed"));
    }
    pool.close().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
