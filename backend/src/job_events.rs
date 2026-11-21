//! PostgreSQL notifications wake SSE streams. The event table owns replay.
use sqlx::{
    PgPool,
    postgres::{PgListener, PgPoolOptions},
};
use std::{sync::OnceLock, time::Duration};
use tokio::{sync::broadcast, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const CHANNEL: &str = "neondojo_job_events";
const RECHECK: Duration = Duration::from_secs(10);

pub(crate) struct JobNotifications {
    // None asks all streams to check their cursor after LISTEN connects.
    signals: broadcast::Sender<Option<Uuid>>,
    task: OnceLock<JoinHandle<()>>,
}

impl Default for JobNotifications {
    fn default() -> Self {
        Self {
            signals: broadcast::channel(256).0,
            task: OnceLock::new(),
        }
    }
}

impl JobNotifications {
    pub(crate) fn subscribe(
        &self,
        pool: &PgPool,
        shutdown: &CancellationToken,
    ) -> broadcast::Receiver<Option<Uuid>> {
        let receiver = self.signals.subscribe();
        self.task.get_or_init(|| {
            let pool = pool.clone();
            let shutdown = shutdown.clone();
            let signals = self.signals.clone();
            tokio::spawn(async move {
                // A long-lived LISTEN connection must not take a request-pool slot.
                let listener_pool = PgPoolOptions::new()
                    .max_connections(1)
                    .acquire_timeout(Duration::from_secs(2))
                    .connect_lazy_with(
                        (*pool.connect_options())
                            .clone()
                            .application_name("neondojo-job-events"),
                    );
                tokio::select! {
                    _ = shutdown.cancelled() => {},
                    _ = pool.close_event() => {},
                    _ = async {
                        loop {
                            if let Err(error) = listen(&listener_pool, &signals).await {
                                tracing::warn!(%error, "Job event listener disconnected");
                            }
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    } => {},
                }
                listener_pool.close().await;
            })
        });
        receiver
    }
}

impl Drop for JobNotifications {
    fn drop(&mut self) {
        if let Some(task) = self.task.get() {
            task.abort();
        }
    }
}

async fn listen(pool: &PgPool, signals: &broadcast::Sender<Option<Uuid>>) -> sqlx::Result<()> {
    let mut listener = PgListener::connect_with(pool).await?;
    listener.eager_reconnect(false);
    listener.listen(CHANNEL).await?;
    // Replay any commits between the previous cursor query and LISTEN.
    let _ = signals.send(None);
    while let Some(notification) = listener.try_recv().await? {
        if let Ok(job) = Uuid::parse_str(notification.payload()) {
            let _ = signals.send(Some(job));
        }
    }
    Ok(())
}

pub(crate) async fn wait(receiver: &mut broadcast::Receiver<Option<Uuid>>, job: Uuid) {
    let relevant = async {
        // A lagged receiver also wakes and replays from its durable cursor.
        while matches!(receiver.recv().await, Ok(Some(other)) if other != job) {}
    };
    let _ = tokio::time::timeout(RECHECK, relevant).await;
}
