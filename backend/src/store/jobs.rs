use crate::model::*;
use anyhow::{Context, Result, ensure};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn get_job(pool: &PgPool, id: Uuid) -> Result<Option<Job>> {
    Ok(sqlx::query_as("SELECT * FROM jobs WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

#[derive(sqlx::FromRow)]
pub(crate) struct JobReviewStatus {
    #[sqlx(flatten)]
    pub job: Job,
    pub restricted: bool,
}

pub(crate) async fn jobs_with_review_status(
    pool: &PgPool,
    ids: &[Uuid],
) -> Result<Vec<JobReviewStatus>> {
    Ok(sqlx::query_as(concat!(
        "SELECT j.*, ",
        crate::play::review_blocked_sql!("coalesce(j.match_uuid,p.match_uuid)"),
        " AS restricted
         FROM jobs j LEFT JOIN positions p ON p.id=j.position_uuid
         WHERE j.id=ANY($1) ORDER BY j.id"
    ))
    .bind(ids)
    .fetch_all(pool)
    .await?)
}

pub async fn create_job(pool: &PgPool, position_id: Uuid, kind: &str, plies: i32) -> Result<Job> {
    create_job_with_snapshot(pool, position_id, kind, plies, None).await
}

pub async fn create_job_with_snapshot(
    pool: &PgPool,
    position_id: Uuid,
    kind: &str,
    plies: i32,
    snapshot: Option<&crate::settings::EngineSnapshot>,
) -> Result<Job> {
    let engine_config = snapshot.map_or_else(|| "legacy".into(), |s| s.fingerprint());
    let snapshot = snapshot.map(serde_json::to_value).transpose()?;
    let mut tx = pool.begin().await?;
    // Position jobs reference positions, so their FK does not itself protect the
    // owning match. Match management takes FOR UPDATE before checking active jobs.
    // Lock the same parent until submission commits to prevent accepted jobs from
    // disappearing in a concurrent match/database cascade.
    let owner = crate::position_retention::lock_owner(&mut tx, position_id).await?;
    ensure!(owner.is_some(), "Position not found");

    let job = loop {
        let inserted = sqlx::query_as::<_, Job>("INSERT INTO jobs(id,position_uuid,kind,plies,status,engine_config,engine_snapshot) VALUES($1,$2,$3,$4,'queued',$5,$6) ON CONFLICT (position_uuid,kind,plies,engine_config) WHERE status IN ('queued','running') DO NOTHING RETURNING *")
            .bind(Uuid::new_v4()).bind(position_id).bind(kind).bind(plies).bind(&engine_config).bind(&snapshot).fetch_optional(&mut *tx).await?;
        if let Some(job) = inserted {
            sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'queued',$2)")
                .bind(job.id)
                .bind(serde_json::json!({"status":"queued","progress":0}))
                .execute(&mut *tx)
                .await?;
            break job;
        }
        let existing = sqlx::query_as("SELECT * FROM jobs WHERE position_uuid=$1 AND kind=$2 AND plies=$3 AND engine_config=$4 AND status IN ('queued','running')")
            .bind(position_id).bind(kind).bind(plies).bind(&engine_config).fetch_optional(&mut *tx).await?;
        if let Some(job) = existing {
            break job;
        }
        // A worker completed the conflicting job after the INSERT snapshot.
        // READ COMMITTED gives the next INSERT a fresh snapshot; try again.
    };
    tx.commit().await?;
    Ok(job)
}

pub async fn events(pool: &PgPool, job_id: Uuid, after: i64) -> Result<Vec<JobEvent>> {
    Ok(
        sqlx::query_as("SELECT * FROM job_events WHERE job_id=$1 AND id>$2 ORDER BY id LIMIT 100")
            .bind(job_id)
            .bind(after)
            .fetch_all(pool)
            .await?,
    )
}

pub async fn create_match_job(
    pool: &PgPool,
    match_id: Uuid,
    plies: i32,
    total_games: i32,
) -> Result<Job> {
    create_match_job_scoped(pool, match_id, plies, total_games, None).await
}

pub async fn create_match_job_scoped(
    pool: &PgPool,
    match_id: Uuid,
    plies: i32,
    total_games: i32,
    game_no: Option<i32>,
) -> Result<Job> {
    create_match_job_scoped_with_snapshot(pool, match_id, plies, total_games, game_no, None).await
}

pub async fn create_match_job_scoped_with_snapshot(
    pool: &PgPool,
    match_id: Uuid,
    plies: i32,
    total_games: i32,
    game_no: Option<i32>,
    snapshot: Option<&crate::settings::EngineSnapshot>,
) -> Result<Job> {
    ensure!(
        game_no.is_none_or(|n| n > 0 && total_games == 1),
        "Invalid game scope"
    );
    let engine_config = snapshot.map_or_else(|| "legacy".into(), |s| s.fingerprint());
    let snapshot = snapshot.map(serde_json::to_value).transpose()?;
    let mut tx = pool.begin().await?;
    let source: Option<bool> =
        sqlx::query_scalar("SELECT NOT collection_only FROM matches WHERE id=$1 FOR SHARE")
            .bind(match_id)
            .fetch_optional(&mut *tx)
            .await?;
    ensure!(
        source == Some(true),
        "Source match is unavailable; analyze the saved position instead"
    );
    // API preflight may precede a live match appending another game. Derive the
    // scope under the same parent lock that protects the job's source snapshot.
    let mut games: Vec<i32> =
        sqlx::query_scalar("SELECT game_no FROM match_games WHERE match_uuid=$1 ORDER BY game_no")
            .bind(match_id)
            .fetch_all(&mut *tx)
            .await?;
    if games.is_empty() {
        let source: String = sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1")
            .bind(match_id)
            .fetch_one(&mut *tx)
            .await?;
        // Parse the source read under the lock, but off the async worker.
        games = tokio::task::spawn_blocking(move || {
            crate::import::sgf::parse_sgf_with_context(&source).map(|(imported, _)| {
                imported
                    .games
                    .into_iter()
                    .map(|game| game.game_no)
                    .collect()
            })
        })
        .await??;
    }
    let total_games = if let Some(game_no) = game_no {
        ensure!(games.contains(&game_no), "Game not found");
        1
    } else {
        i32::try_from(games.len())?
    };
    ensure!(total_games > 0, "Match contains no games");
    let job = loop {
        let inserted = sqlx::query_as::<_, Job>("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,game_no,engine_config,engine_snapshot) VALUES($1,$2,'match',$3,'queued',$4,$5,$6,$7) ON CONFLICT(match_uuid,(coalesce(game_no,0)),plies,engine_config) WHERE kind='match' AND status IN ('queued','running') DO NOTHING RETURNING *")
            .bind(Uuid::new_v4()).bind(match_id).bind(plies).bind(total_games).bind(game_no).bind(&engine_config).bind(&snapshot).fetch_optional(&mut *tx).await?;
        if let Some(job) = inserted {
            sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'queued',$2)")
                .bind(job.id).bind(serde_json::json!({"status":"queued","progress":0,"total_games":total_games,"completed_games":0}))
                .execute(&mut *tx).await?;
            break job;
        }
        let existing = sqlx::query_as("SELECT * FROM jobs WHERE match_uuid=$1 AND plies=$2 AND game_no IS NOT DISTINCT FROM $3 AND engine_config=$4 AND kind='match' AND status IN ('queued','running')")
            .bind(match_id).bind(plies).bind(game_no).bind(&engine_config).fetch_optional(&mut *tx).await?;
        if let Some(job) = existing {
            break job;
        }
    };
    tx.commit().await?;
    Ok(job)
}

/// A legacy row has no snapshot and intentionally retains the injected engine.
/// New and recovered jobs use exactly the installation saved at submission time.
/// Read after the worker registers for cancellation so a revoked claim cannot
/// start an engine even if its cancellation signal preceded registration.
pub(crate) async fn claimed_job_engine(
    pool: &PgPool,
    job_id: Uuid,
    lease: Uuid,
    fallback: &crate::engine::Engine,
) -> Result<crate::engines::AnalysisEngine> {
    let snapshot: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT engine_snapshot FROM jobs WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
            .bind(job_id)
            .bind(lease)
            .fetch_optional(pool)
            .await?
            .context("Job lease was lost")?;
    match snapshot {
        Some(snapshot) => {
            serde_json::from_value::<crate::settings::EngineSnapshot>(snapshot)?.engine()
        }
        None => Ok(crate::engines::AnalysisEngine::Gnubg(fallback.clone())),
    }
}
