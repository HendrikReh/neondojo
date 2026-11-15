//! Versioned, derived checker projections for indexed saved-position filters.
//! Original imported records stay immutable; Rust projection owns every rule.
use crate::{
    api::{ApiError, ApiResult},
    imported_analysis::ImportedAnalysisData,
    move_catalog::StoredMove,
    recurring_mistakes::imported_projection,
};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

pub(crate) const REVISION: i32 = 1;
const BATCH: usize = 2_000;

pub(crate) async fn ensure_saved(pool: &PgPool, database_id: Option<Uuid>) -> ApiResult<()> {
    // Legacy source catalogs are prepared only for saved hosts in this scope.
    let legacy: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT m.id FROM saved_positions saved JOIN positions p ON p.id=saved.position_uuid JOIN matches m ON m.id=p.match_uuid
         WHERE ($1::uuid IS NULL OR coalesce(m.source_database_id,m.database_id)=$1)
           AND (NOT m.moves_cataloged OR NOT m.imported_analysis_loaded) ORDER BY m.id LIMIT 17",
    ).bind(database_id).fetch_all(pool).await.map_err(anyhow::Error::from)?;
    let more_legacy = legacy.len() > 16;
    for id in legacy.into_iter().take(16) {
        crate::move_catalog::ensure_catalog(pool, id).await?;
        crate::imported_analysis::ensure_catalog(pool, id).await?;
    }
    if more_legacy {
        return Err(ApiError::invalid(
            "Indexed 16 legacy saved sources; retry or choose a database to continue",
        ));
    }
    #[derive(FromRow)]
    struct Row {
        id: Uuid,
        data: Value,
        format: String,
        checker_ply: i32,
        source_revision: i64,
        event: Option<sqlx::types::Json<StoredMove>>,
    }
    let rows:Vec<Row> = sqlx::query_as(
        "SELECT ia.id,ia.data,m.format,m.source_revision,p.ply AS checker_ply,to_jsonb(e) AS event
         FROM saved_positions saved JOIN positions p ON p.id=saved.position_uuid JOIN matches m ON m.id=p.match_uuid
         JOIN primary_imported_analyses ia ON ia.position_uuid=p.id AND ia.match_uuid=m.id AND ia.kind='checker'
         LEFT JOIN imported_analysis_projections cached ON cached.imported_analysis_id=ia.id AND cached.revision=$2 AND cached.source_revision=m.source_revision
         LEFT JOIN match_moves e ON e.match_uuid=m.id AND e.game_no=ia.game_no AND e.move_no=ia.source_move_no AND e.position_uuid=p.id
         WHERE ($1::uuid IS NULL OR coalesce(m.source_database_id,m.database_id)=$1) AND cached.imported_analysis_id IS NULL
         ORDER BY ia.id LIMIT 2001",
    ).bind(database_id).bind(REVISION).fetch_all(pool).await.map_err(anyhow::Error::from)?;
    let more = rows.len() > BATCH;
    let projected = tokio::task::spawn_blocking(move || {
        rows.into_iter().take(BATCH).map(|row| {
            let projection = row.event.and_then(|event| {
                let record=serde_json::from_value::<ImportedAnalysisData>(row.data).ok()?;
                let source=imported_projection::source_move(&event.0,Some(row.checker_ply))?;
                let projected=imported_projection::project(&record,&source,event.match_uuid,event.position_uuid,&row.format)?;
                Some(serde_json::json!({"decision":projected.decision,"depth":projected.depth,"plies":projected.depth_plies}))
            });
            (row.id,row.source_revision,projection)
        }).collect::<Vec<_>>()
    }).await.map_err(anyhow::Error::from)?;
    if !projected.is_empty() {
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "INSERT INTO imported_analysis_projections(imported_analysis_id,revision,source_revision,projection) ",
        );
        query.push_values(projected, |mut row, (id, source_revision, projection)| {
            row.push_bind(id)
                .push_bind(REVISION)
                .push_bind(source_revision)
                .push_bind(projection);
        });
        query.push(" ON CONFLICT(imported_analysis_id) DO UPDATE SET revision=EXCLUDED.revision,source_revision=EXCLUDED.source_revision,projection=EXCLUDED.projection WHERE imported_analysis_projections.source_revision<=EXCLUDED.source_revision");
        query
            .build()
            .execute(pool)
            .await
            .map_err(anyhow::Error::from)?;
    }
    if more {
        return Err(ApiError::invalid(
            "Indexed 2000 saved imported analyses; retry or choose a database to continue",
        ));
    }
    Ok(())
}
