//! Shared access gate coordinating ordinary engine subprocesses and exclusive benchmarks.

use std::time::Duration;

// Every application-created engine subprocess shares this gate. The benchmark
// owns the writer for its entire run; its task-local scope avoids recursive reads.
static ENGINE_WORK: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());
tokio::task_local! { static ENGINE_ACCESS: (); }

pub(super) async fn engine_permit() -> Option<tokio::sync::RwLockReadGuard<'static, ()>> {
    if ENGINE_ACCESS.try_with(|_| ()).is_ok() {
        None
    } else {
        Some(ENGINE_WORK.read().await)
    }
}

/// Acquire the ordinary shared gate before entering a timed pipeline. The task
/// scope lets nested version/export commands reuse it even if a writer queues.
pub(crate) async fn with_shared_access<F: std::future::Future>(work: F) -> F::Output {
    let _permit = engine_permit().await;
    ENGINE_ACCESS.scope((), work).await
}

/// Hold admission through the entire Play startup. A pending benchmark writer
/// also rejects new sessions; nested subprocesses reuse the outer read guard.
pub(crate) async fn try_play_start<F: std::future::Future>(work: F) -> Option<F::Output> {
    let _admission = ENGINE_WORK.try_read().ok()?;
    Some(ENGINE_ACCESS.scope((), work).await)
}

/// Bound a request's total time, including admission to the shared engine gate.
pub(crate) async fn with_shared_access_timeout<F: std::future::Future>(
    timeout: Duration,
    work: F,
) -> Result<F::Output, tokio::time::error::Elapsed> {
    tokio::time::timeout(timeout, with_shared_access(work)).await
}

pub(crate) async fn benchmark_exclusive<F: std::future::Future>(work: F) -> F::Output {
    let _exclusive = ENGINE_WORK.write().await;
    ENGINE_ACCESS.scope((), work).await
}
