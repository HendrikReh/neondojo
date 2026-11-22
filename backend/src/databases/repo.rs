use super::*;

pub(crate) async fn require_library_destination(pool: &PgPool, id: Uuid) -> ApiResult<()> {
    if id == PAUSED_MATCHES_DATABASE_ID {
        return Err(conflict(
            "Only paused sessions can be added to Paused Matches",
        ));
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM databases WHERE id=$1 AND NOT collection_only)",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(anyhow::Error::from)?;
    if !exists {
        return Err(ApiError::missing());
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) enum DatabaseRemoval {
    Clear,
    Delete { delete_matches: bool },
}

pub(super) async fn remove_database_content(
    pool: &PgPool,
    id: Uuid,
    expected_version: i64,
    removal: DatabaseRemoval,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    let d: Option<(i64, bool)> = sqlx::query_as(
        "SELECT version,is_system FROM databases WHERE id=$1 AND NOT collection_only FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let Some((version, is_system)) = d else {
        return Err(ApiError::missing());
    };
    match removal {
        DatabaseRemoval::Delete { .. } if is_system => {
            return Err(conflict("System databases cannot be deleted"));
        }
        DatabaseRemoval::Clear if !is_system => {
            return Err(conflict("Only system databases can be cleared"));
        }
        DatabaseRemoval::Clear if id == PAUSED_MATCHES_DATABASE_ID => {
            return Err(conflict("Paused Matches cannot be cleared"));
        }
        _ => {}
    }
    if version != expected_version {
        return Err(conflict("Database changed; reload before deleting"));
    }
    // Match locks serialize deletion with bulk operations and job submission FK checks.
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM matches WHERE database_id=$1 ORDER BY id FOR UPDATE")
            .bind(id)
            .fetch_all(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    if !ids.is_empty()
        && matches!(
            removal,
            DatabaseRemoval::Delete {
                delete_matches: false
            }
        )
    {
        return Err(conflict(
            "Database is not empty; delete_matches=true is required",
        ));
    }
    if any_active(&mut tx, &ids).await? {
        return Err(conflict("Database has queued or running analyses"));
    }
    for &match_id in &ids {
        crate::position_retention::preserve(&mut tx, match_id).await?;
    }
    sqlx::query("DELETE FROM matches WHERE database_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let finish = match removal {
        DatabaseRemoval::Clear => {
            "UPDATE databases SET version=version+1,updated_at=now() WHERE id=$1"
        }
        DatabaseRemoval::Delete { .. } => "DELETE FROM databases WHERE id=$1",
    };
    sqlx::query(finish)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Library rows for `ids`, in the order of `ids`; missing matches are omitted.
pub(crate) async fn database_matches(
    pool: &PgPool,
    ids: &[Uuid],
) -> anyhow::Result<Vec<DatabaseMatch>> {
    let rows: Vec<DatabaseMatch> = sqlx::query_as(&format!(
        "SELECT {MATCH_COLUMNS} FROM matches m{MATCH_PLAY_JOIN}{MATCH_NOTE_JOIN}{MATCH_ANALYSIS_JOINS} WHERE m.id=ANY($1)"
    ))
    .bind(ids)
    .fetch_all(pool)
    .await?;
    let mut rows: HashMap<Uuid, DatabaseMatch> = rows.into_iter().map(|m| (m.id, m)).collect();
    Ok(ids.iter().filter_map(|id| rows.remove(id)).collect())
}

pub(super) async fn database_match(pool: &PgPool, id: Uuid) -> ApiResult<DatabaseMatch> {
    sqlx::query_as(&format!(
        "SELECT {MATCH_COLUMNS} FROM matches m{MATCH_PLAY_JOIN}{MATCH_NOTE_JOIN}{MATCH_ANALYSIS_JOINS} WHERE m.id=$1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or_else(ApiError::missing)
}
