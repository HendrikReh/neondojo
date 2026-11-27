use crate::model::*;
use anyhow::Result;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

pub async fn latest_match_job_scoped(
    pool: &PgPool,
    match_id: Uuid,
    game_no: Option<i32>,
    engine_id: Option<&str>,
) -> Result<Option<Job>> {
    let mut query = QueryBuilder::new("SELECT * FROM jobs");
    query
        .push(" WHERE source_revision=(SELECT source_revision FROM matches WHERE id=")
        .push_bind(match_id)
        .push(") AND match_uuid=")
        .push_bind(match_id)
        .push(" AND game_no IS NOT DISTINCT FROM ")
        .push_bind(game_no)
        .push(" AND kind='match'");
    if let Some(engine_id) = engine_id {
        query
            .push(" AND coalesce(engine_snapshot->>'engine_id','gnubg')=")
            .push_bind(engine_id);
    }
    query.push(" ORDER BY created_at DESC,id DESC LIMIT 1");
    Ok(query.build_query_as().fetch_optional(pool).await?)
}

pub async fn match_analysis_jobs_for_scope(
    pool: &PgPool,
    match_id: Uuid,
    game_no: Option<i32>,
) -> Result<Vec<AnalysisComparisonRun>> {
    // The latest run in each state keeps completed results accessible even
    // during a rerun and bounds the response by provider, depth and status.
    Ok(sqlx::query_as(
        "SELECT * FROM (
            SELECT DISTINCT ON (coalesce(j.engine_snapshot->>'engine_id','gnubg'), j.plies, j.status, (j.game_no IS NULL))
                j.*, CASE WHEN j.game_no IS NULL THEN c.summary END AS game_summary
            FROM jobs j JOIN matches m ON m.id=j.match_uuid AND m.source_revision=j.source_revision
            LEFT JOIN match_analysis_games c ON c.job_id=j.id AND c.game_no=$2 AND j.game_no IS NULL
            WHERE j.match_uuid=$1 AND j.kind='match'
              AND NOT m.collection_only
              AND (coalesce(j.game_no,0)=coalesce($2::integer,0)
                   OR (c.job_id IS NOT NULL AND j.status='completed' AND j.completed_games=j.total_games))
            ORDER BY coalesce(j.engine_snapshot->>'engine_id','gnubg'), j.plies, j.status,
                     (j.game_no IS NULL), (c.summary IS NOT NULL) DESC, j.created_at DESC, j.id DESC
        ) history ORDER BY created_at DESC, id DESC",
    )
    .bind(match_id)
    .bind(game_no)
    .fetch_all(pool)
    .await?)
}

pub async fn move_analyses(
    pool: &PgPool,
    job_id: Uuid,
    after: i64,
    limit: i64,
    errors_only: bool,
    player: Option<i32>,
    kind: Option<&str>,
) -> Result<MoveAnalysisPage> {
    let thresholds = crate::configuration::store::error_thresholds(pool).await?;
    move_analyses_with_thresholds(
        pool,
        job_id,
        after,
        limit,
        errors_only,
        player,
        kind,
        None,
        None,
        thresholds,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn move_analyses_with_thresholds(
    pool: &PgPool,
    job_id: Uuid,
    after: i64,
    limit: i64,
    errors_only: bool,
    player: Option<i32>,
    kind: Option<&str>,
    game_no: Option<i32>,
    severity: Option<&str>,
    thresholds: crate::configuration::ErrorThresholds,
) -> Result<MoveAnalysisPage> {
    let mut q = QueryBuilder::<Postgres>::new(
        "SELECT id,position_uuid,game_no,data,grade FROM (SELECT id,position_uuid,game_no,data,",
    );
    thresholds.push_classification(&mut q, "equity_loss", "data->>'evaluation_status'");
    q.push(" AS grade FROM move_analyses WHERE job_id=");

    q.push_bind(job_id).push(" AND id>").push_bind(after);
    if let Some(game_no) = game_no {
        q.push(" AND game_no=").push_bind(game_no);
    }
    if errors_only {
        q.push(" AND equity_loss >= ")
            .push_bind(thresholds.questionable);
    }
    if let Some(player) = player {
        q.push(" AND player=").push_bind(player);
    }
    if let Some(kind) = kind {
        q.push(" AND kind=").push_bind(kind);
    }
    q.push(") AS moves WHERE TRUE");
    if errors_only {
        q.push(" AND grade <> 'none'");
    }
    if let Some(severity) = severity {
        q.push(" AND grade=").push_bind(severity);
    }
    q.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let rows = q
        .build_query_as::<(i64, Option<Uuid>, i32, serde_json::Value, String)>()
        .fetch_all(pool)
        .await?;
    let mut items: Vec<MoveAnalysis> = rows
        .into_iter()
        .map(|(id, position_uuid, game_no, data, severity)| {
            let mut decision: crate::match_analysis::Decision = serde_json::from_value(data)?;
            decision.severity = severity;
            Ok(MoveAnalysis {
                id,
                position_uuid,
                game_no,
                decision,
            })
        })
        .collect::<Result<_>>()?;
    let next_cursor = if items.len() > limit as usize {
        items.pop();
        items.last().map(|row| row.id)
    } else {
        None
    };
    Ok(MoveAnalysisPage { items, next_cursor })
}
