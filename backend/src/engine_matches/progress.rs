use super::Series;
use crate::play::engine_match::{DuelUnit, GameProgress};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use utoipa::ToSchema;
use uuid::Uuid;

const UPDATE_INTERVAL: Duration = Duration::from_secs(2);
const WRITE_TIMEOUT: Duration = Duration::from_millis(500);

/// The current unit's provisional score, in series A/B order.
#[derive(Debug, Clone, Copy, FromRow, Serialize, ToSchema)]
pub struct LiveProgress {
    /// One-based index of the match or money block, counting both legs.
    pub unit_number: i32,
    /// One-based game within the unit.
    pub game_number: i32,
    pub completed_games: i32,
    pub score_a: i32,
    pub score_b: i32,
    /// A wall-clock estimate from the current worker attempt, once it has samples.
    pub estimated_remaining_seconds: Option<i64>,
}

pub(super) async fn load(pool: &PgPool, series: &Series) -> sqlx::Result<Option<LiveProgress>> {
    if series.status != "running" {
        return Ok(None);
    }
    sqlx::query_as(
        "SELECT p.* FROM engine_match_progress p JOIN engine_match_runs r ON r.id=p.run_id
         WHERE r.id=$1 AND r.status='running' AND r.lease_until>now()
           AND p.lease_token=r.lease_token AND p.unit_number=r.completed_matches+1
           AND r.completed_matches=$2",
    )
    .bind(series.id)
    .bind(series.completed_matches)
    .fetch_optional(pool)
    .await
}

pub(super) struct ProgressReporter<'a> {
    pool: &'a PgPool,
    updates: Option<&'a broadcast::Sender<Uuid>>,
    series: &'a Series,
    token: Uuid,
    unit: DuelUnit,
    started: Instant,
    resumed_games: i32,
    last_update: Option<Instant>,
}

impl<'a> ProgressReporter<'a> {
    pub fn new(
        pool: &'a PgPool,
        updates: Option<&'a broadcast::Sender<Uuid>>,
        series: &'a Series,
        token: Uuid,
        unit: DuelUnit,
    ) -> Self {
        Self {
            pool,
            updates,
            series,
            token,
            unit,
            started: Instant::now(),
            resumed_games: 0,
            last_update: None,
        }
    }

    pub async fn start_unit(&mut self, index: i32, swapped: bool, initial: GameProgress) {
        if index == self.series.completed_matches {
            self.resumed_games = initial.completed_games;
        }
        self.last_update = None;
        self.publish(index, swapped, initial).await;
    }

    pub fn publish(
        &mut self,
        index: i32,
        swapped: bool,
        game: GameProgress,
    ) -> impl Future<Output = ()> + use<'a> {
        let now = Instant::now();
        let should_update = self
            .last_update
            .is_none_or(|last| now - last >= UPDATE_INTERVAL);
        let write = should_update.then(|| {
            self.last_update = Some(now);
            let [score_a, score_b] = if swapped {
                [game.scores[1], game.scores[0]]
            } else {
                game.scores
            };
            let progress = LiveProgress {
                unit_number: index + 1,
                game_number: game.completed_games + i32::from(!game.unit_finished),
                completed_games: game.completed_games,
                score_a,
                score_b,
                estimated_remaining_seconds: estimate(
                    self.unit,
                    (self.series.completed_matches, self.resumed_games),
                    self.series.total_matches,
                    index,
                    game,
                    now - self.started,
                ),
            };
            store(
                self.pool,
                self.updates,
                self.series.id,
                self.token,
                progress,
            )
        });
        async move {
            if let Some(write) = write {
                write.await;
            }
        }
    }
}

fn estimate(
    unit: DuelUnit,
    (attempt_start, resumed_games): (i32, i32),
    total: i32,
    index: i32,
    game: GameProgress,
    elapsed: Duration,
) -> Option<i64> {
    let (completed, remaining) = match unit {
        DuelUnit::Match { .. } => {
            let done = index + i32::from(game.unit_finished);
            (done - attempt_start, total - done)
        }
        DuelUnit::MoneyCubeless { games } => (
            (index - attempt_start) * games + game.completed_games - resumed_games,
            (total - index) * games - game.completed_games,
        ),
    };
    (completed > 0 && remaining > 0).then(|| {
        (elapsed.as_secs_f64() / f64::from(completed) * f64::from(remaining)).ceil() as i64
    })
}

async fn store(
    pool: &PgPool,
    updates: Option<&broadcast::Sender<Uuid>>,
    id: Uuid,
    token: Uuid,
    progress: LiveProgress,
) {
    // Lock the run before writing so cancellation or a new lease cannot race
    // a stale worker's upsert. A delayed progress write can be skipped.
    let write = sqlx::query(
        "WITH owned AS MATERIALIZED (
             SELECT id,lease_until FROM engine_match_runs
             WHERE id=$1 AND lease_token=$2 AND status='running' AND completed_matches=$3-1 FOR UPDATE
         )
         INSERT INTO engine_match_progress
             (run_id,lease_token,unit_number,game_number,completed_games,score_a,score_b,estimated_remaining_seconds)
         SELECT id,$2,$3,$4,$5,$6,$7,$8 FROM owned WHERE lease_until>clock_timestamp()
         ON CONFLICT (run_id) DO UPDATE SET
             lease_token=excluded.lease_token, unit_number=excluded.unit_number,
             game_number=excluded.game_number, completed_games=excluded.completed_games,
             score_a=excluded.score_a, score_b=excluded.score_b,
             estimated_remaining_seconds=excluded.estimated_remaining_seconds",
    )
    .bind(id)
    .bind(token)
    .bind(progress.unit_number)
    .bind(progress.game_number)
    .bind(progress.completed_games)
    .bind(progress.score_a)
    .bind(progress.score_b)
    .bind(progress.estimated_remaining_seconds)
    .execute(pool);
    match tokio::time::timeout(WRITE_TIMEOUT, write).await {
        Ok(Ok(result)) if result.rows_affected() > 0 => {
            if let Some(tx) = updates {
                let _ = tx.send(id);
            }
        }
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            tracing::debug!(series = %id, %error, "Could not store live duel progress")
        }
        Err(_) => tracing::debug!(series = %id, "Timed out storing live duel progress"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_estimate_uses_games_within_the_block_and_only_times_this_attempt() {
        let unit = DuelUnit::MoneyCubeless { games: 1_000 };
        let elapsed = Duration::from_secs(60);
        let game = GameProgress {
            completed_games: 100,
            ..Default::default()
        };
        assert_eq!(estimate(unit, (2, 0), 10, 2, game, elapsed), Some(4_740));
        assert_eq!(estimate(unit, (2, 100), 10, 2, game, elapsed), None);
        assert_eq!(estimate(unit, (2, 50), 10, 2, game, elapsed), Some(9_480));
        assert_eq!(
            estimate(unit, (2, 0), 10, 2, GameProgress::default(), elapsed),
            None
        );
    }

    #[test]
    fn match_estimate_waits_for_a_match_and_includes_the_current_match() {
        let unit = DuelUnit::Match { length: 5 };
        let elapsed = Duration::from_secs(60);
        let game = GameProgress {
            completed_games: 2,
            ..Default::default()
        };
        assert_eq!(estimate(unit, (0, 0), 10, 0, game, elapsed), None);
        assert_eq!(estimate(unit, (0, 0), 10, 1, game, elapsed), Some(540));
        let last_game = GameProgress {
            unit_finished: true,
            ..game
        };
        assert_eq!(estimate(unit, (0, 0), 10, 0, last_game, elapsed), Some(540));
        assert_eq!(estimate(unit, (0, 0), 10, 9, last_game, elapsed), None);
    }
}
