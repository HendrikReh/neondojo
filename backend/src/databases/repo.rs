use super::*;

pub(crate) async fn library_database_exists<'e>(
    db: impl sqlx::PgExecutor<'e>,
    id: Uuid,
) -> ApiResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM databases WHERE id=$1 AND NOT collection_only)",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(anyhow::Error::from)?)
}

pub(crate) async fn require_library_database<'e>(
    db: impl sqlx::PgExecutor<'e>,
    id: Uuid,
) -> ApiResult<()> {
    if !library_database_exists(db, id).await? {
        return Err(ApiError::missing());
    }
    Ok(())
}

pub(crate) async fn require_library_database_invalid<'e>(
    db: impl sqlx::PgExecutor<'e>,
    id: Uuid,
    message: &str,
) -> ApiResult<()> {
    if !library_database_exists(db, id).await? {
        return Err(ApiError::invalid(message));
    }
    Ok(())
}

pub(crate) async fn require_library_destination(pool: &PgPool, id: Uuid) -> ApiResult<()> {
    if id == PAUSED_MATCHES_DATABASE_ID {
        return Err(conflict(
            "Only paused sessions can be added to Paused Matches",
        ));
    }
    require_library_database(pool, id).await
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

/// Read the same analysis status as the Library page, limited to visible matches
/// affected by a notification. A reconnect reconciles all supplied rows.
pub(crate) async fn library_analysis_statuses(
    pool: &PgPool,
    match_ids: &[Uuid],
    changed_job: Option<Uuid>,
) -> anyhow::Result<Vec<(Uuid, Option<String>)>> {
    let mut query = QueryBuilder::new("SELECT m.id,analysis.status FROM matches m");
    query
        .push(MATCH_PLAY_JOIN)
        .push(MATCH_ANALYSIS_JOINS)
        .push(" WHERE m.id=ANY(")
        .push_bind(match_ids)
        .push(")");
    if let Some(id) = changed_job {
        query.push(" AND m.id=(SELECT match_uuid FROM jobs WHERE kind='match' AND game_no IS NULL AND id=")
            .push_bind(id).push(")");
    }
    Ok(query.build_query_as().fetch_all(pool).await?)
}

pub(crate) async fn library_bulk_job_statuses(
    pool: &PgPool,
    ids: &[Uuid],
) -> anyhow::Result<Vec<LibraryBulkJobStatus>> {
    Ok(sqlx::query_as(concat!(
        "SELECT requested.id,coalesce(j.status,'missing') AS status,
                coalesce(j.progress,0) AS progress, coalesce(j.completed_games,0) AS completed_games,
                coalesce(j.total_games,0) AS total_games
         FROM unnest($1::uuid[]) AS requested(id)
         LEFT JOIN jobs j ON j.id=requested.id AND j.kind='match' AND j.game_no IS NULL AND NOT ",
        crate::play::review_blocked_sql!("j.match_uuid")
    ))
    .bind(ids)
    .fetch_all(pool)
    .await?)
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

pub(super) async fn list_library_databases(pool: &PgPool) -> ApiResult<Vec<Database>> {
    Ok(sqlx::query_as(&format!(
        "{DB_SELECT} WHERE NOT d.collection_only ORDER BY is_default DESC,lower(name),id"
    ))
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?)
}

async fn database_by_id(pool: &PgPool, id: Uuid) -> ApiResult<Database> {
    Ok(sqlx::query_as(&format!("{DB_SELECT} WHERE d.id=$1"))
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(anyhow::Error::from)?)
}

pub(super) async fn insert_database(
    pool: &PgPool,
    name: &str,
    description: &str,
) -> ApiResult<Database> {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name,description) VALUES($1,$2,$3)")
        .bind(id)
        .bind(name)
        .bind(description)
        .execute(pool)
        .await
        .map_err(anyhow::Error::from)?;
    database_by_id(pool, id).await
}

pub(super) async fn write_database(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    description: &str,
    version: i64,
) -> ApiResult<Database> {
    let affected = sqlx::query(
        "UPDATE databases SET name=$2,description=$3,version=version+1,updated_at=now()
         WHERE id=$1 AND version=$4 AND NOT collection_only AND (NOT is_system OR name=$2)",
    )
    .bind(id)
    .bind(name)
    .bind(description)
    .bind(version)
    .execute(pool)
    .await
    .map_err(anyhow::Error::from)?
    .rows_affected();
    if affected == 0 {
        let renamed_system: Option<bool> = sqlx::query_scalar(
            "SELECT is_system AND name<>$2 FROM databases WHERE id=$1 AND NOT collection_only",
        )
        .bind(id)
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(anyhow::Error::from)?;
        return Err(match renamed_system {
            Some(true) => conflict("System databases cannot be renamed"),
            Some(false) => conflict("Database changed; reload before saving"),
            None => ApiError::missing(),
        });
    }
    database_by_id(pool, id).await
}

pub(super) async fn write_match_metadata(
    pool: &PgPool,
    id: Uuid,
    w: MetadataWrite,
) -> ApiResult<()> {
    let affected = sqlx::query(
        "UPDATE matches SET display_name=$2,played_on=$3,tournament=$4,round=$5,place=$6,
             result_state=$7,white_score=$8,black_score=$9,metadata_version=metadata_version+1,
             source_metadata_loaded=true,native_match_context_pending=false
         WHERE id=$1 AND metadata_version=$10 AND NOT collection_only",
    )
    .bind(id)
    .bind(w.display_name.trim())
    .bind(w.played_on)
    .bind(w.tournament)
    .bind(w.round)
    .bind(w.place)
    .bind(w.result_state)
    .bind(w.white_score)
    .bind(w.black_score)
    .bind(w.version)
    .execute(pool)
    .await
    .map_err(anyhow::Error::from)?
    .rows_affected();
    if affected == 0 {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM matches WHERE id=$1 AND NOT collection_only)",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(anyhow::Error::from)?;
        return Err(if exists {
            conflict("Metadata changed; reload before saving")
        } else {
            ApiError::missing()
        });
    }
    Ok(())
}

type ExportFile = (Uuid, String, Vec<u8>);

#[derive(sqlx::FromRow)]
pub(super) struct XgSource {
    pub(super) id: Uuid,
    pub(super) filename: String,
    pub(super) sgf: String,
    pub(super) played_on: Option<NaiveDate>,
    pub(super) tournament: Option<String>,
    pub(super) round: Option<String>,
    pub(super) place: Option<String>,
}

pub(super) enum ExportSources {
    Files(Vec<ExportFile>),
    Xg(Vec<XgSource>),
}

pub(super) async fn load_export_sources(
    pool: &PgPool,
    ids: &[Uuid],
    format: ExportFormat,
) -> ApiResult<ExportSources> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    let original = format == ExportFormat::Original;
    let sizes: Vec<(Uuid, i64)> = sqlx::query_as(if original {
        "SELECT id,octet_length(original)::bigint FROM matches WHERE id=ANY($1) AND NOT collection_only ORDER BY id FOR SHARE"
    } else {
        "SELECT id,octet_length(sgf)::bigint FROM matches WHERE id=ANY($1) AND NOT collection_only ORDER BY id FOR SHARE"
    })
    .bind(ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if sizes.len() != ids.len() {
        return Err(ApiError::missing());
    }
    if sizes.iter().map(|(_, bytes)| bytes).sum::<i64>() > MAX_EXPORT_BYTES {
        return Err(ApiError::invalid("Export exceeds 64 MiB"));
    }
    let sources = if format == ExportFormat::Xg {
        ExportSources::Xg(
            sqlx::query_as(
                "SELECT id,filename,sgf,played_on,tournament,round,place FROM matches WHERE id=ANY($1) ORDER BY id",
            )
            .bind(ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?,
        )
    } else {
        ExportSources::Files(
            sqlx::query_as(if original {
                "SELECT id,filename,original FROM matches WHERE id=ANY($1) ORDER BY id"
            } else {
                "SELECT id,filename,convert_to(sgf,'UTF8') FROM matches WHERE id=ANY($1) ORDER BY id"
            })
            .bind(ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?,
        )
    };
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(sources)
}
