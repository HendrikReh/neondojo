//! Internal Tokio <-> GPUI executor bridge.
//!
//! NeonDojo's backend (`sqlx`, `PgListener`, `tokio::process`, and
//! `tokio::task_local! ENGINE_ACCESS`) requires a Tokio multi-threaded runtime,
//! while GPUI runs its own foreground/background executors. Routing all backend
//! futures through [`TokioBridge`] keeps the UI thread non-blocking and
//! distinguishes immediately-abortable read queries ([`ReadTask`]) from
//! non-abortable write mutations.

use gpui_kit::{App, Global, Task};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
    time::Duration,
};
use tokio::{
    runtime::{Handle, Runtime},
    task::{AbortHandle, JoinError},
};

/// Default graceful shutdown budget when the GPUI application exits.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

struct OwnedRuntime {
    runtime: Option<Runtime>,
    handle: Handle,
}

impl Drop for OwnedRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(SHUTDOWN_TIMEOUT);
        }
    }
}

/// An abortable Tokio read task bridged into GPUI.
///
/// Dropping [`ReadTask`] calls [`AbortHandle::abort`] synchronously on the
/// caller's thread without waiting for GPUI's executor to dequeue a cancelled
/// runnable, replacing browser `AbortController` guards reliably across both
/// production and headless [`gpui_kit::TestAppContext`] runs.
pub struct ReadTask<R> {
    task: Task<Result<R, JoinError>>,
    abort: AbortHandle,
}

impl<R> Drop for ReadTask<R> {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

impl<R: Unpin + 'static> Future for ReadTask<R> {
    type Output = Result<R, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.task).poll(cx)
    }
}

/// Global handle to NeonDojo's multi-threaded Tokio runtime inside GPUI.
#[derive(Clone)]
pub struct TokioBridge {
    inner: Arc<OwnedRuntime>,
}

impl Global for TokioBridge {}

impl TokioBridge {
    /// Initializes a dedicated multi-threaded Tokio runtime and registers it as
    /// a GPUI [`Global`]. Safe to call once during app startup.
    pub fn init(cx: &mut App) -> anyhow::Result<()> {
        if cx.has_global::<Self>() {
            return Ok(());
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("neondojo-tokio")
            .enable_all()
            .build()?;
        let handle = runtime.handle().clone();
        cx.set_global(Self {
            inner: Arc::new(OwnedRuntime {
                runtime: Some(runtime),
                handle,
            }),
        });
        Ok(())
    }

    /// Registers an existing Tokio [`Handle`] without taking ownership of the
    /// underlying runtime (useful in tests).
    pub fn init_with_handle(cx: &mut App, handle: Handle) {
        cx.set_global(Self {
            inner: Arc::new(OwnedRuntime {
                runtime: None,
                handle,
            }),
        });
    }

    /// Returns the registered Tokio runtime [`Handle`].
    pub fn handle(cx: &App) -> Handle {
        cx.global::<Self>().inner.handle.clone()
    }

    /// Spawns an **abortable read query** onto the Tokio runtime.
    pub fn spawn_read<Fut, R>(cx: &App, fut: Fut) -> ReadTask<R>
    where
        Fut: Future<Output = R> + Send + 'static,
        R: Send + 'static,
    {
        let join_handle = Self::handle(cx).spawn(fut);
        let abort = join_handle.abort_handle();
        let task = cx.background_executor().spawn(join_handle);
        ReadTask { task, abort }
    }

    /// Spawns a **non-abortable write mutation** onto the Tokio runtime.
    ///
    /// Dropping the returned GPUI [`Task`] detaches observation without
    /// cancelling the backend write (required for Play turns, FSRS reviews,
    /// exports, and database restores).
    pub fn spawn_write<Fut, R>(cx: &App, fut: Fut) -> Task<Result<R, JoinError>>
    where
        Fut: Future<Output = R> + Send + 'static,
        R: Send + 'static,
    {
        let join_handle = Self::handle(cx).spawn(fut);
        cx.background_executor().spawn(join_handle)
    }
}
