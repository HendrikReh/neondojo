//! Temporary directories whose creation and recursive removal can block.
use anyhow::Result;
use std::path::Path;
use tempfile::TempDir;
use tokio::runtime::Handle;

pub(crate) struct AsyncTempDir {
    directory: Option<TempDir>,
    runtime: Handle,
}

impl AsyncTempDir {
    pub(crate) async fn new() -> Result<Self> {
        // Return the guard itself so cancellation after creation also schedules cleanup.
        tokio::task::spawn_blocking(|| {
            Ok(Self {
                directory: Some(tempfile::tempdir()?),
                runtime: Handle::current(),
            })
        })
        .await?
    }

    pub(crate) fn path(&self) -> &Path {
        self.directory.as_ref().expect("workspace is live").path()
    }
}

impl Drop for AsyncTempDir {
    fn drop(&mut self) {
        if let Some(directory) = self.directory.take() {
            self.runtime.spawn_blocking(move || drop(directory));
        }
    }
}
