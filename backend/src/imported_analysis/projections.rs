//! Versioned, derived checker projections for indexed saved-position filters.
//! Original imported records stay immutable; Rust projection owns every rule.
use crate::{
    imported_analysis::ImportedAnalysisData, move_catalog::StoredMove,
    recurring_mistakes::imported_projection,
};
use serde_json::Value;
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

pub(crate) const REVISION: i32 = 2;
const BATCH: usize = 2_000;

const LEGACY: &str = "FROM saved_positions saved JOIN positions p ON p.id=saved.position_uuid
    JOIN matches m ON m.id=p.match_uuid WHERE (NOT m.moves_cataloged OR NOT m.imported_analysis_loaded)";

// OFFSET 0 keeps the primary-record lookup local to each saved position.
const SOURCES: &str = "FROM saved_positions saved JOIN positions p ON p.id=saved.position_uuid
    JOIN matches m ON m.id=p.match_uuid
    JOIN LATERAL (SELECT primary_record.* FROM primary_imported_analyses primary_record
        WHERE primary_record.position_uuid=p.id AND primary_record.match_uuid=m.id AND primary_record.kind='checker' OFFSET 0) ia ON true
    LEFT JOIN imported_analysis_projections cached ON cached.imported_analysis_id=ia.id
        AND cached.revision=$1 AND cached.source_revision=m.source_revision";
const IN_DATABASE: &str = "($2::uuid IS NULL OR coalesce(m.source_database_id,m.database_id)=$2)";

pub(crate) fn pending_sql() -> String {
    format!(
        "SELECT EXISTS(SELECT 1 {LEGACY} AND {IN_DATABASE})
        OR EXISTS(SELECT 1 {SOURCES} WHERE cached.imported_analysis_id IS NULL AND {IN_DATABASE})"
    )
}

pub(crate) async fn pending(conn: &mut PgConnection, database: Option<Uuid>) -> sqlx::Result<bool> {
    sqlx::query_scalar(&pending_sql())
        .bind(REVISION)
        .bind(database)
        .fetch_one(conn)
        .await
}

/// Keep bookmark membership available even when an old source cannot be parsed.
async fn catalog_source(pool: &PgPool, id: Uuid) {
    let catalog = async {
        crate::move_catalog::ensure_catalog(pool, id).await?;
        crate::imported_analysis::ensure_catalog(pool, id).await
    };
    if let Err(error) = catalog.await {
        tracing::warn!(%id, %error, "Failed to catalog a saved position source");
    }
}

pub(crate) async fn prepare_bookmark_source(pool: &PgPool, position: Uuid) -> sqlx::Result<()> {
    let source = sqlx::query_scalar(
        "SELECT m.id FROM positions p JOIN matches m ON m.id=p.match_uuid
         WHERE p.id=$1 AND (NOT m.moves_cataloged OR NOT m.imported_analysis_loaded)",
    )
    .bind(position)
    .fetch_optional(pool)
    .await?;
    if let Some(id) = source {
        catalog_source(pool, id).await;
    }
    Ok(())
}

pub(crate) async fn backfill_legacy(
    pool: &PgPool,
    after: &mut Option<Uuid>,
) -> anyhow::Result<bool> {
    let ids: Vec<Uuid> = sqlx::query_scalar(&format!(
        "SELECT DISTINCT m.id {LEGACY}
        AND ($1::uuid IS NULL OR m.id>$1) ORDER BY m.id LIMIT 16"
    ))
    .bind(*after)
    .fetch_all(pool)
    .await?;
    let full = ids.len() == 16;
    *after = if full { ids.last().copied() } else { None };
    for id in ids {
        // A corrupt legacy source must not keep other matches from being cataloged.
        catalog_source(pool, id).await;
    }
    Ok(full)
}

pub(crate) async fn refresh(
    conn: &mut PgConnection,
    position: Option<Uuid>,
) -> anyhow::Result<bool> {
    #[derive(FromRow)]
    struct Row {
        cube_enabled: Option<bool>,
        id: Uuid,
        data: Value,
        format: crate::import::MatchFormat,
        checker_ply: i32,
        source_revision: i64,
        event: Option<sqlx::types::Json<StoredMove>>,
    }
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "SELECT ia.id,ia.data,m.format,m.source_revision,g.cube_enabled,p.ply AS checker_ply,to_jsonb(e) AS event
         {SOURCES}
         LEFT JOIN match_games g ON g.match_uuid=p.match_uuid AND g.game_no=p.game_no
         LEFT JOIN match_moves e ON e.match_uuid=m.id AND e.game_no=ia.game_no AND e.move_no=ia.source_move_no AND e.position_uuid=p.id
         WHERE m.moves_cataloged AND m.imported_analysis_loaded AND cached.imported_analysis_id IS NULL AND ($2::uuid IS NULL OR p.id=$2)
         ORDER BY ia.id LIMIT $3"))
        .bind(REVISION).bind(position).bind(BATCH as i64).fetch_all(&mut *conn).await?;
    if rows.is_empty() {
        return Ok(false);
    }
    let full = rows.len() == BATCH;
    let projected = tokio::task::spawn_blocking(move || {
        rows.into_iter().map(|row| {
            let projection = row.event.and_then(|event| {
                let record=serde_json::from_value::<ImportedAnalysisData>(row.data).ok()?;
                let source=imported_projection::source_move(&event.0,Some(row.checker_ply))?;
                let projected=imported_projection::project(&record,&source,event.match_uuid,event.position_uuid,row.format,row.cube_enabled)?;
                Some(serde_json::json!({"decision":projected.decision,"depth":projected.depth,"plies":projected.depth_plies}))
            });
            (row.id,row.source_revision,projection)
        }).collect::<Vec<_>>()
    }).await?;

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
    query.build().execute(conn).await?;
    Ok(full)
}
