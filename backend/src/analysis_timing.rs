//! Durable confirmed work and read-only adaptive job estimates.
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    model::{Job, JobStatus},
    store,
};
use anyhow::{Result, ensure};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemainingTimeState {
    Queued,
    Waiting,
    Estimated,
    Reestimating,
    Unavailable,
    Finishing,
    Completed,
    Failed,
}
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemainingTimeBasis {
    Benchmark,
    Observed,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct AnalysisRemainingTime {
    pub job_id: Uuid,
    #[schema(value_type=JobStatus)]
    pub status: String,
    pub state: RemainingTimeState,
    pub basis: Option<RemainingTimeBasis>,
    pub remaining_ms: Option<f64>,
    pub total_moves: i64,
    pub completed_moves: i64,
    pub timed_moves: i64,
    pub measured_elapsed_ms: f64,
    pub current_game_elapsed_ms: f64,
    pub sampled_at: DateTime<Utc>,
    pub attempt: i32,
}
#[derive(sqlx::FromRow)]
struct Sample {
    job_id: Uuid,
    status: String,
    attempt: i32,
    total_games: i32,
    completed_games: i32,
    ready: bool,
    total_moves: i64,
    completed_moves: i64,
    timed_moves: i64,
    measured_elapsed_ms: f64,
    benchmark_average_move_ms: Option<f64>,
    current_game_elapsed_ms: f64,
    active: bool,
    sampled_at: DateTime<Utc>,
}
impl Sample {
    fn estimate(self) -> AnalysisRemainingTime {
        let observed = (self.timed_moves > 0 && self.measured_elapsed_ms > 0.0)
            .then(|| self.measured_elapsed_ms / self.timed_moves as f64);
        let rate = observed
            .or(self.benchmark_average_move_ms)
            .filter(|r| r.is_finite() && *r > 0.0);
        let basis = rate.map(|_| {
            if observed.is_some() {
                RemainingTimeBasis::Observed
            } else {
                RemainingTimeBasis::Benchmark
            }
        });
        let remaining = rate
            .map(|r| {
                r * (self.total_moves - self.completed_moves).max(0) as f64
                    - self.current_game_elapsed_ms
            })
            .filter(|r| r.is_finite() && *r > 0.0);
        let state = match self.status.as_str() {
            "queued" => RemainingTimeState::Queued,
            "completed" => RemainingTimeState::Completed,
            "failed" => RemainingTimeState::Failed,
            _ if self.completed_games >= self.total_games => RemainingTimeState::Finishing,
            _ if !self.ready => RemainingTimeState::Unavailable,
            _ if !self.active => RemainingTimeState::Waiting,
            _ if rate.is_none() => RemainingTimeState::Unavailable,
            _ if remaining.is_none() => RemainingTimeState::Reestimating,
            _ => RemainingTimeState::Estimated,
        };
        let live = matches!(
            state,
            RemainingTimeState::Waiting
                | RemainingTimeState::Estimated
                | RemainingTimeState::Reestimating
                | RemainingTimeState::Unavailable
        );
        AnalysisRemainingTime {
            job_id: self.job_id,
            status: self.status,
            state,
            basis: if live { basis } else { None },
            remaining_ms: if live { remaining } else { None },
            total_moves: self.total_moves,
            completed_moves: self.completed_moves,
            timed_moves: self.timed_moves,
            measured_elapsed_ms: self.measured_elapsed_ms,
            current_game_elapsed_ms: self.current_game_elapsed_ms,
            sampled_at: self.sampled_at,
            attempt: self.attempt,
        }
    }
}
pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/jobs/{id}/remaining-time", get(remaining_time))
}
#[utoipa::path(get,path="/api/jobs/{id}/remaining-time",params(("id"=Uuid,Path)),responses((status=200,body=AnalysisRemainingTime),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Match analysis")]
async fn remaining_time(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AnalysisRemainingTime>> {
    let job = store::get_job(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_job_review_available(&state.pool, &job).await?;
    if job.kind != "match" {
        return Err(ApiError::invalid("This job is not a match analysis"));
    }
    let source = store::get_match(&state.pool, job.match_uuid.ok_or_else(ApiError::missing)?)
        .await?
        .ok_or_else(ApiError::missing)?;
    if !source.source_available {
        return Err(ApiError::invalid(
            "The source match was removed; analyze the saved position instead",
        ));
    }
    let sample: Sample = sqlx::query_as(r#"
        WITH sample AS (
            SELECT j.id AS job_id, j.status, j.attempts AS attempt,
                j.total_games, j.completed_games, t.job_id IS NOT NULL AS ready,
                COALESCE(t.total_moves, (
                    SELECT count(*) FROM positions p WHERE p.match_uuid=j.match_uuid
                    AND (j.game_no IS NULL OR p.game_no=j.game_no)
                )) AS total_moves,
                COALESCE(t.completed_moves, (j.result->>'analyzed_moves')::bigint, 0) AS completed_moves,
                COALESCE(t.timed_moves, 0) AS timed_moves,
                COALESCE(t.measured_elapsed_ms, 0) AS measured_elapsed_ms,
                t.benchmark_average_move_ms, COALESCE(t.active_elapsed_ms, 0) AS active_elapsed_ms,
                COALESCE(j.status='running' AND j.lease_until>statement_timestamp()
                    AND t.lease_token=j.lease_token AND t.attempt=j.attempts
                    AND t.active_started_at IS NOT NULL, false) AS active,
                statement_timestamp() AS sampled_at
            FROM jobs j LEFT JOIN match_analysis_timing t ON t.job_id=j.id WHERE j.id=$1
        )
        SELECT *, CASE WHEN active
            THEN active_elapsed_ms
            ELSE 0::float8 END AS current_game_elapsed_ms
        FROM sample
        "#)
        .bind(id).fetch_optional(&state.pool).await.map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    Ok(Json(sample.estimate()))
}
#[derive(OpenApi)]
#[openapi(
    paths(remaining_time),
    components(schemas(AnalysisRemainingTime, RemainingTimeState, RemainingTimeBasis))
)]
pub struct TimingDoc;

pub(crate) async fn initialize(
    pool: &PgPool,
    job: &Job,
    token: Uuid,
    engine: &crate::engines::AnalysisEngine,
    version: &str,
    total_moves: i32,
    completed_moves: i32,
) -> Result<()> {
    let snapshot: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT engine_snapshot FROM jobs WHERE id=$1")
            .bind(job.id)
            .fetch_one(pool)
            .await?;
    let engine_id = snapshot
        .as_ref()
        .and_then(|s| s["engine_id"].as_str())
        .unwrap_or("gnubg");
    let path = if engine_id == "bgsage" {
        Ok(engine.binary().to_owned())
    } else {
        crate::engine::resolve_binary(engine.binary())
    };
    let benchmark = match (engine, path) {
        (crate::engines::AnalysisEngine::Gnubg(engine), _) if engine.calculation_threads() != 1 => {
            None
        }
        (_, Ok(path)) => {
            let move_filter = match engine {
                crate::engines::AnalysisEngine::Gnubg(engine) => engine.move_filter(),
                crate::engines::AnalysisEngine::OpenSage(_) => None,
            };
            crate::benchmarks::estimate::matching_benchmark(
                pool,
                engine_id,
                &path,
                version,
                job.plies,
                move_filter,
            )
            .await?
        }
        (_, Err(_)) => None,
    };
    let mut tx = pool.begin().await?;
    crate::match_jobs::assert_lease(&mut tx, job.id, token).await?;
    sqlx::query("INSERT INTO match_analysis_timing(job_id,total_moves,completed_moves,benchmark_id,benchmark_average_move_ms,lease_token,attempt) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(job_id) DO UPDATE SET lease_token=EXCLUDED.lease_token,attempt=EXCLUDED.attempt,active_started_at=NULL,active_elapsed_ms=0")
        .bind(job.id).bind(i64::from(total_moves)).bind(i64::from(completed_moves)).bind(benchmark.as_ref().map(|b| b.id)).bind(benchmark.as_ref().map(|b| b.average_move_ms)).bind(token).bind(job.attempts).execute(&mut *tx).await?;
    touch(&mut tx, job.id, token).await?;
    tx.commit().await?;
    Ok(())
}
pub(crate) async fn start_game(pool: &PgPool, job: &Job, token: Uuid) -> Result<()> {
    let mut tx = pool.begin().await?;
    crate::match_jobs::assert_lease(&mut tx, job.id, token).await?;
    let changed = sqlx::query("UPDATE match_analysis_timing SET active_started_at=clock_timestamp(),active_elapsed_ms=0 WHERE job_id=$1 AND lease_token=$2 AND attempt=$3")
        .bind(job.id).bind(token).bind(job.attempts).execute(&mut *tx).await?;
    ensure!(
        changed.rows_affected() == 1,
        "Job timing ownership was lost"
    );
    touch(&mut tx, job.id, token).await?;
    tx.commit().await?;
    Ok(())
}
/// Persist only a monotonic duration; wall-clock timestamps are ownership/start
/// metadata and never become elapsed analysis time. Run concurrently with work
/// so a checkpoint's row lock cannot prevent that checkpoint from committing.
pub(crate) async fn sample_game(
    pool: &PgPool,
    job: &Job,
    token: Uuid,
    elapsed_ms: f64,
) -> Result<()> {
    let changed = sqlx::query(
        r#"
        UPDATE match_analysis_timing t SET active_elapsed_ms=GREATEST(active_elapsed_ms,$4)
        WHERE t.job_id=$1 AND t.lease_token=$2 AND t.attempt=$3 AND t.active_started_at IS NOT NULL
        AND EXISTS(SELECT 1 FROM jobs j WHERE j.id=t.job_id AND j.lease_token=$2
            AND j.attempts=$3 AND j.status='running' AND j.lease_until>statement_timestamp())
        "#,
    )
    .bind(job.id)
    .bind(token)
    .bind(job.attempts)
    .bind(elapsed_ms)
    .execute(pool)
    .await?;
    if changed.rows_affected() == 0 {
        // The game checkpoint may have cleared active state while this update
        // waited for its row lock. That is benign only for the live owner.
        let owns: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM jobs WHERE id=$1 AND lease_token=$2 AND attempts=$3 AND status='running' AND lease_until>statement_timestamp())")
            .bind(job.id).bind(token).bind(job.attempts).fetch_one(pool).await?;
        ensure!(owns, "Job lease was lost");
    }
    Ok(())
}
async fn touch(tx: &mut Transaction<'_, Postgres>, id: Uuid, token: Uuid) -> Result<()> {
    sqlx::query("UPDATE jobs SET updated_at=clock_timestamp() WHERE id=$1 AND lease_token=$2")
        .bind(id)
        .bind(token)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO job_events(job_id,event,data) SELECT id,'progress',jsonb_build_object('status',status,'progress',progress,'attempt',attempts,'completed_games',completed_games,'total_games',total_games) FROM jobs WHERE id=$1").bind(id).execute(&mut **tx).await?;
    Ok(())
}
/// The checkpoint caller holds the live job row lock from assert_lease.
pub(crate) async fn confirm_game(
    tx: &mut Transaction<'_, Postgres>,
    job: &Job,
    token: Uuid,
    moves: i64,
    elapsed_ms: f64,
) -> Result<()> {
    let changed = sqlx::query("UPDATE match_analysis_timing SET completed_moves=completed_moves+$4,timed_moves=timed_moves+$4,measured_elapsed_ms=measured_elapsed_ms+$5,active_started_at=NULL,active_elapsed_ms=0 WHERE job_id=$1 AND lease_token=$2 AND attempt=$3")
        .bind(job.id).bind(token).bind(job.attempts).bind(moves).bind(elapsed_ms).execute(&mut **tx).await?;
    ensure!(
        changed.rows_affected() == 1,
        "Job timing ownership was lost"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[sqlx::test(migrations = "./migrations")]
    async fn timing_writes_are_fenced_and_retry_preserves_confirmed_work(pool: PgPool) {
        let sgf = "(;GM[6]FF[4]MI[length:7][ws:0][bs:0];W[31hefe])";
        let imported = store::import_match(
            &pool,
            "timing.sgf",
            "sgf",
            sgf.as_bytes(),
            sgf,
            &crate::import::parse_sgf(sgf).unwrap(),
        )
        .await
        .unwrap();
        let job = store::create_match_job(&pool, imported.r#match.id, 1, 1)
            .await
            .unwrap();
        let token = Uuid::new_v4();
        sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+interval '120 seconds' WHERE id=$1").bind(job.id).bind(token).execute(&pool).await.unwrap();
        let job = store::get_job(&pool, job.id).await.unwrap().unwrap();
        let engine: crate::engines::AnalysisEngine =
            crate::engine::Engine::new("/no-process-needed".into()).into();
        assert!(
            initialize(
                &pool,
                &job,
                Uuid::new_v4(),
                &engine,
                "GNU Backgammon test",
                1,
                0
            )
            .await
            .is_err()
        );
        initialize(&pool, &job, token, &engine, "GNU Backgammon test", 1, 0)
            .await
            .unwrap();
        assert!(start_game(&pool, &job, Uuid::new_v4()).await.is_err());
        start_game(&pool, &job, token).await.unwrap();
        sample_game(&pool, &job, token, 1000.0).await.unwrap();
        sample_game(&pool, &job, token, 500.0).await.unwrap();
        let sampled: f64 = sqlx::query_scalar(
            "SELECT active_elapsed_ms FROM match_analysis_timing WHERE job_id=$1",
        )
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            sampled, 1000.0,
            "a saved monotonic duration never goes backwards"
        );
        assert!(
            sample_game(&pool, &job, Uuid::new_v4(), 9000.0)
                .await
                .is_err()
        );
        let mut tx = pool.begin().await.unwrap();
        assert!(
            confirm_game(&mut tx, &job, Uuid::new_v4(), 1, 1500.0)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        crate::match_jobs::assert_lease(&mut tx, job.id, token)
            .await
            .unwrap();
        confirm_game(&mut tx, &job, token, 1, 1500.0).await.unwrap();
        let mut pending_sample = Box::pin(sample_game(&pool, &job, token, 9000.0));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut pending_sample)
                .await
                .is_err(),
            "sampler waits for the confirmation row lock"
        );
        tx.commit().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), pending_sample)
            .await
            .expect("confirmation must release the sampler without deadlocking")
            .expect("normal active clear must not fail the completed game");
        let cleared: f64 = sqlx::query_scalar(
            "SELECT active_elapsed_ms FROM match_analysis_timing WHERE job_id=$1",
        )
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            cleared, 0.0,
            "a blocked sampler must not revive a confirmed section"
        );
        start_game(&pool, &job, token).await.unwrap();
        let next_token = Uuid::new_v4();
        sqlx::query("UPDATE jobs SET attempts=2,lease_token=$2 WHERE id=$1")
            .bind(job.id)
            .bind(next_token)
            .execute(&pool)
            .await
            .unwrap();
        let next = store::get_job(&pool, job.id).await.unwrap().unwrap();
        initialize(
            &pool,
            &next,
            next_token,
            &engine,
            "GNU Backgammon test",
            1,
            1,
        )
        .await
        .unwrap();
        assert!(start_game(&pool, &job, token).await.is_err());
        let saved:(i64,i64,f64,Option<DateTime<Utc>>)=sqlx::query_as("SELECT completed_moves,timed_moves,measured_elapsed_ms,active_started_at FROM match_analysis_timing WHERE job_id=$1").bind(job.id).fetch_one(&pool).await.unwrap();
        assert_eq!(saved, (1, 1, 1500.0, None));
        // Neither interrupted current work nor a replaced worker can add time.
        let mut tx = pool.begin().await.unwrap();
        assert!(confirm_game(&mut tx, &job, token, 0, 9999.0).await.is_err());
        tx.rollback().await.unwrap();
    }
}
