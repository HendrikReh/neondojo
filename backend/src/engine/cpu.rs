//! CPU admission for native evaluations. Idle sidecars and GNUbg rules
//! processes hold no permits while another engine evaluates their position.
use anyhow::{Context, Result};
use tokio::sync::{OnceCell, Semaphore, SemaphorePermit};

#[derive(Clone, Copy)]
pub(crate) enum Workload {
    Background,
    Foreground,
}

pub(crate) struct HostCpuBudget {
    capacity: u32,
    background_capacity: u32,
    all: Semaphore,
    background: Semaphore,
}

impl HostCpuBudget {
    fn new(capacity: u32) -> Self {
        let capacity = capacity.max(1);
        let background_capacity = capacity.saturating_sub(2).max(1);
        Self {
            capacity,
            background_capacity,
            all: Semaphore::new(capacity as usize),
            background: Semaphore::new(background_capacity as usize),
        }
    }

    pub(crate) async fn host() -> &'static Self {
        static HOST: OnceCell<HostCpuBudget> = OnceCell::const_new();
        HOST.get_or_init(|| async {
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            if let Ok(output) = tokio::process::Command::new("/usr/sbin/sysctl")
                .args(["-n", "hw.perflevel0.physicalcpu"])
                .output()
                .await
                && output.status.success()
                && let Ok(n) = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u32>()
                && n > 0
            {
                return Self::new(n);
            }
            Self::new(
                std::thread::available_parallelism()
                    .ok()
                    .and_then(|n| u32::try_from(n.get()).ok())
                    .unwrap_or(1),
            )
        })
        .await
    }

    pub(crate) async fn acquire(&self, threads: u32, workload: Workload) -> Result<CpuPermit<'_>> {
        let mut count = threads.clamp(1, self.capacity);
        let background = match workload {
            Workload::Background => {
                // Saved profiles retain their thread count. If the profile
                // needs the foreground reserve, run its exchange alone.
                let seats = count.min(self.background_capacity);
                if count > self.background_capacity {
                    count = self.capacity;
                }
                Some(
                    self.background
                        .acquire_many(seats)
                        .await
                        .context("Engine CPU budget closed")?,
                )
            }
            Workload::Foreground => None,
        };
        let total = self
            .all
            .acquire_many(count)
            .await
            .context("Engine CPU budget closed")?;
        Ok(CpuPermit {
            _total: total,
            _background: background,
        })
    }

    pub(crate) fn capacity(&self) -> u32 {
        self.capacity
    }

    pub(crate) async fn host_parallel_threads() -> Option<i32> {
        i32::try_from(Self::host().await.capacity()).ok()
    }
}

pub(crate) struct CpuPermit<'a> {
    _total: SemaphorePermit<'a>,
    _background: Option<SemaphorePermit<'a>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::poll;
    #[tokio::test]
    async fn background_jobs_share_capacity_and_leave_two_seats_for_play() {
        let budget = HostCpuBudget::new(10);
        let gnubg = budget.acquire(4, Workload::Background).await.unwrap();
        let sage = budget.acquire(4, Workload::Background).await.unwrap();
        let more = budget.acquire(1, Workload::Background);
        tokio::pin!(more);
        assert!(poll!(&mut more).is_pending());
        let play = budget.acquire(2, Workload::Foreground).await.unwrap();
        let next_play = budget.acquire(1, Workload::Foreground);
        tokio::pin!(next_play);
        assert!(poll!(&mut next_play).is_pending());
        drop(sage);
        let _more = more.await.unwrap();
        let _next_play = next_play.await.unwrap();
        drop((gnubg, play));
    }

    #[tokio::test]
    async fn wide_profiles_keep_their_configured_search_and_run_alone() {
        for capacity in [1, 2, 10] {
            let budget = HostCpuBudget::new(capacity);
            let play = budget.acquire(1, Workload::Foreground).await.unwrap();
            let wide = budget.acquire(capacity + 2, Workload::Background);
            tokio::pin!(wide);
            assert!(poll!(&mut wide).is_pending());
            drop(play);
            let wide = wide.await.unwrap();
            assert!(poll!(Box::pin(budget.acquire(1, Workload::Foreground))).is_pending());
            drop(wide);
        }
    }
}
