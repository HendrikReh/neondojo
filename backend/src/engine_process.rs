//! Bounded process cleanup shared by local engine adapters.
use anyhow::{Context, Result};
use std::time::Duration;
use tokio::process::Child;

/// try_wait also works when the runtime that originally spawned a child ended.
pub(crate) async fn reap_child(child: &mut Child) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while child
            .try_wait()
            .context("Cannot reap engine process")?
            .is_none()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    })
    .await
    .context("Timed out reaping engine process")?
}

pub(crate) fn terminate_and_reap(mut child: Child) {
    let _ = child.start_kill();
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(async move {
            let _ = reap_child(&mut child).await;
        });
    }
}
