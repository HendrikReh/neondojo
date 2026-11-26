use super::*;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/databases", get(list_databases).post(create_database))
        .route(
            "/api/databases/{id}",
            axum::routing::put(update_database).delete(delete_database),
        )
        .route(
            "/api/databases/{id}/matches",
            axum::routing::delete(clear_database),
        )
        .route("/api/library/matches", get(library_matches))
        .route(
            "/api/matches/{id}/metadata",
            get(get_metadata)
                .put(update_metadata)
                .patch(update_metadata),
        )
        .route("/api/library/actions", post(actions))
        .route("/api/library/export", post(export))
}

#[utoipa::path(get,path="/api/databases",responses((status=200,body=[Database])),tag="Databases")]
pub(super) async fn list_databases(State(s): State<AppState>) -> ApiResult<Json<Vec<Database>>> {
    Ok(Json(
        sqlx::query_as(&format!(
            "{DB_SELECT} WHERE NOT d.collection_only ORDER BY is_default DESC,lower(name),id"
        ))
        .fetch_all(&s.pool)
        .await
        .map_err(anyhow::Error::from)?,
    ))
}

#[utoipa::path(post,path="/api/databases",request_body=DatabaseWrite,responses((status=201,body=Database),(status=422,body=ErrorBody)),tag="Databases")]
pub(super) async fn create_database(
    State(s): State<AppState>,
    Json(w): Json<DatabaseWrite>,
) -> ApiResult<(StatusCode, Json<Database>)> {
    validate_database(&w)?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name,description) VALUES($1,$2,$3)")
        .bind(id)
        .bind(w.name.trim())
        .bind(w.description)
        .execute(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
    Ok((
        StatusCode::CREATED,
        Json(
            sqlx::query_as(&format!("{DB_SELECT} WHERE d.id=$1"))
                .bind(id)
                .fetch_one(&s.pool)
                .await
                .map_err(anyhow::Error::from)?,
        ),
    ))
}

#[utoipa::path(put,path="/api/databases/{id}",params(("id"=Uuid,Path)),request_body=DatabaseWrite,responses((status=200,body=Database),(status=409,body=ErrorBody)),tag="Databases")]
pub(super) async fn update_database(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(w): Json<DatabaseWrite>,
) -> ApiResult<Json<Database>> {
    validate_database(&w)?;
    let version = w
        .version
        .filter(|v| *v > 0)
        .ok_or_else(|| ApiError::invalid("version is required"))?;
    let name = w.name.trim();
    let affected = sqlx::query(
        "UPDATE databases SET name=$2,description=$3,version=version+1,updated_at=now()
         WHERE id=$1 AND version=$4 AND NOT collection_only AND (NOT is_system OR name=$2)",
    )
    .bind(id)
    .bind(name)
    .bind(w.description)
    .bind(version)
    .execute(&s.pool)
    .await
    .map_err(anyhow::Error::from)?
    .rows_affected();
    if affected == 0 {
        let renamed_system: Option<bool> = sqlx::query_scalar(
            "SELECT is_system AND name<>$2 FROM databases WHERE id=$1 AND NOT collection_only",
        )
        .bind(id)
        .bind(name)
        .fetch_optional(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
        return Err(match renamed_system {
            Some(true) => conflict("System databases cannot be renamed"),
            Some(false) => conflict("Database changed; reload before saving"),
            None => ApiError::missing(),
        });
    }
    Ok(Json(
        sqlx::query_as(&format!("{DB_SELECT} WHERE d.id=$1"))
            .bind(id)
            .fetch_one(&s.pool)
            .await
            .map_err(anyhow::Error::from)?,
    ))
}

#[utoipa::path(delete,path="/api/databases/{id}",params(("id"=Uuid,Path),DatabaseDelete),responses((status=204),(status=409,body=ErrorBody)),tag="Databases")]
pub(super) async fn delete_database(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<DatabaseDelete>,
) -> ApiResult<StatusCode> {
    remove_database_content(
        &s.pool,
        id,
        q.version,
        DatabaseRemoval::Delete {
            delete_matches: q.delete_matches,
        },
    )
    .await
}

#[utoipa::path(delete,path="/api/databases/{id}/matches",params(("id"=Uuid,Path),DatabaseVersion),responses((status=204),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Databases")]
pub(super) async fn clear_database(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<DatabaseVersion>,
) -> ApiResult<StatusCode> {
    remove_database_content(&s.pool, id, q.version, DatabaseRemoval::Clear).await
}

#[utoipa::path(get,path="/api/library/matches",params(LibraryQuery),responses((status=200,body=LibraryPage),(status=422,body=ErrorBody)),tag="Databases")]
pub(super) async fn library_matches(
    State(s): State<AppState>,
    Query(q): Query<LibraryQuery>,
) -> ApiResult<Json<LibraryPage>> {
    Ok(Json(browse(&s.pool, q).await?))
}

#[utoipa::path(get,path="/api/matches/{id}/metadata",params(("id"=Uuid,Path)),responses((status=200,body=DatabaseMatch),(status=404,body=ErrorBody)),tag="Databases")]
pub(super) async fn get_metadata(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<DatabaseMatch>> {
    Ok(Json(database_match(&s.pool, id).await?))
}

#[utoipa::path(put,path="/api/matches/{id}/metadata",params(("id"=Uuid,Path)),request_body=MetadataWrite,responses((status=200,body=DatabaseMatch),(status=409,body=ErrorBody)),tag="Databases")]
pub(super) async fn update_metadata(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(w): Json<MetadataWrite>,
) -> ApiResult<Json<DatabaseMatch>> {
    if w.version < 0
        || w.display_name.chars().count() > 240
        || w.tournament
            .as_ref()
            .is_some_and(|v| v.chars().count() > 240)
        || w.round.as_ref().is_some_and(|v| v.chars().count() > 120)
        || w.place.as_ref().is_some_and(|v| v.chars().count() > 240)
        || !["unknown", "ongoing", "completed"].contains(&w.result_state.as_str())
        || w.white_score.is_some_and(|v| v < 0)
        || w.black_score.is_some_and(|v| v < 0)
    {
        return Err(ApiError::invalid("Invalid match metadata"));
    }
    let affected=sqlx::query("UPDATE matches SET display_name=$2,played_on=$3,tournament=$4,round=$5,place=$6,result_state=$7,white_score=$8,black_score=$9,metadata_version=metadata_version+1,source_metadata_loaded=true,native_match_context_pending=false WHERE id=$1 AND metadata_version=$10 AND NOT collection_only").bind(id).bind(w.display_name.trim()).bind(w.played_on).bind(w.tournament).bind(w.round).bind(w.place).bind(w.result_state).bind(w.white_score).bind(w.black_score).bind(w.version).execute(&s.pool).await.map_err(anyhow::Error::from)?.rows_affected();
    if affected == 0 {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM matches WHERE id=$1 AND NOT collection_only)",
        )
        .bind(id)
        .fetch_one(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
        return Err(if exists {
            conflict("Metadata changed; reload before saving")
        } else {
            ApiError::missing()
        });
    }
    s.browse_wake.notify_one();
    Ok(Json(database_match(&s.pool, id).await?))
}

#[utoipa::path(post,path="/api/library/actions",request_body=LibraryAction,responses((status=200,body=ActionResponse),(status=409,body=ErrorBody,description="Target is reserved for paused sessions"),(status=422,body=ErrorBody)),tag="Databases")]
pub(super) async fn actions(
    State(s): State<AppState>,
    Json(mut a): Json<LibraryAction>,
) -> ApiResult<Json<ActionResponse>> {
    if a.match_ids.is_empty() || a.match_ids.len() > MAX_ACTIONS {
        return Err(ApiError::invalid("Select between 1 and 100 matches"));
    }
    if !["copy", "move", "delete", "tags"].contains(&a.action.as_str()) {
        return Err(ApiError::invalid("Unknown library action"));
    }
    if ["copy", "move"].contains(&a.action.as_str()) {
        let db = a
            .target_database_id
            .ok_or_else(|| ApiError::invalid("target_database_id is required"))?;
        require_library_destination(&s.pool, db).await?;
    }
    a.add_tags = a
        .add_tags
        .iter()
        .map(|s| normalize_tag(s))
        .collect::<ApiResult<Vec<_>>>()?
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    a.remove_tags = a
        .remove_tags
        .iter()
        .map(|s| normalize_tag(s))
        .collect::<ApiResult<Vec<_>>>()?;
    let mut seen = HashSet::new();
    let mut results = Vec::new();
    for &id in &a.match_ids {
        if !seen.insert(id) {
            results.push(action_item(id, "duplicate_request", None, None));
            continue;
        }
        results.push(action_one(&s.pool, &a, id).await?)
    }
    s.browse_wake.notify_one();
    Ok(Json(ActionResponse { results }))
}

#[utoipa::path(post,path="/api/library/export",request_body=ExportRequest,responses((status=200,description="Original, SGF or XG file, or a ZIP for multiple matches",content_type="application/octet-stream"),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Databases")]
pub(super) async fn export(
    State(s): State<AppState>,
    Json(r): Json<ExportRequest>,
) -> ApiResult<Response> {
    if r.match_ids.is_empty() || r.match_ids.len() > MAX_EXPORTS {
        return Err(ApiError::invalid("Select between 1 and 50 matches"));
    }
    if !["original", "sgf", "xg"].contains(&r.format.as_str()) {
        return Err(ApiError::invalid("format must be original, sgf or xg"));
    }
    let mut seen = HashSet::new();
    if r.match_ids.iter().any(|id| !seen.insert(*id)) {
        return Err(ApiError::invalid("Duplicate match IDs are not allowed"));
    }
    let mut tx = s.pool.begin().await.map_err(anyhow::Error::from)?;
    let sizes:Vec<(Uuid,i64)>=sqlx::query_as(if r.format=="original"{"SELECT id,octet_length(original)::bigint FROM matches WHERE id=ANY($1) AND NOT collection_only ORDER BY id FOR SHARE"}else{"SELECT id,octet_length(sgf)::bigint FROM matches WHERE id=ANY($1) AND NOT collection_only ORDER BY id FOR SHARE"}).bind(&r.match_ids).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
    if sizes.len() != r.match_ids.len() {
        return Err(ApiError::missing());
    }
    if sizes.iter().map(|(_, n)| n).sum::<i64>() > MAX_EXPORT_BYTES {
        return Err(ApiError::invalid("Export exceeds 64 MiB"));
    }
    let files: Vec<(Uuid, String, Vec<u8>)> = if r.format == "xg" {
        type XgSource = (
            Uuid,
            String,
            String,
            Option<NaiveDate>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let sources: Vec<XgSource> = sqlx::query_as(
            "SELECT id,filename,sgf,played_on,tournament,round,place FROM matches WHERE id=ANY($1) ORDER BY id",
        )
        .bind(&r.match_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        tx.commit().await.map_err(anyhow::Error::from)?;
        // The conversion replays every checker play, so it runs off the async
        // workers after the shared row locks are released.
        tokio::task::spawn_blocking(move || {
            sources
                .into_iter()
                .map(|(id, name, sgf, played_on, event, round, place)| {
                    let meta = crate::import::XgExportMetadata {
                        match_id: id,
                        played_on,
                        event: event.as_deref(),
                        round: round.as_deref(),
                        place: place.as_deref(),
                    };
                    match crate::import::sgf_to_xg(&sgf, &meta) {
                        Ok(bytes) => Ok((id, name, bytes)),
                        Err(error) => Err(ApiError::invalid(format!(
                            "{name} cannot be exported as XG: {error:#}"
                        ))),
                    }
                })
                .collect::<Result<Vec<_>, ApiError>>()
        })
        .await
        .map_err(anyhow::Error::from)??
    } else {
        let files = sqlx::query_as(if r.format == "original" {
            "SELECT id,filename,original FROM matches WHERE id=ANY($1) ORDER BY id"
        } else {
            "SELECT id,filename,convert_to(sgf,'UTF8') FROM matches WHERE id=ANY($1) ORDER BY id"
        })
        .bind(&r.match_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        tx.commit().await.map_err(anyhow::Error::from)?;
        files
    };
    let safe_name = |id: Uuid, name: &str| {
        let name = name.rsplit(['/', '\\']).next().unwrap_or("match");
        let clean: String = name
            .chars()
            .map(|c| {
                if c.is_control() || "\"<>:|?*".contains(c) {
                    '_'
                } else {
                    c
                }
            })
            .take(180)
            .collect();
        let clean = if clean.trim_matches('.').is_empty() {
            "match".to_string()
        } else {
            clean
        };
        if r.format != "original" {
            let stem = clean.rsplit_once('.').map(|(s, _)| s).unwrap_or(&clean);
            format!("{stem}-{id}.{}", r.format)
        } else {
            format!("{id}-{clean}")
        }
    };
    let (name, bytes, content_type) = if files.len() == 1 {
        let (id, name, data) = files.into_iter().next().unwrap();
        (safe_name(id, &name), data, "application/octet-stream")
    } else {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (id, name, data) in files {
            zip.start_file(
                safe_name(id, &name),
                zip::write::SimpleFileOptions::default(),
            )
            .map_err(anyhow::Error::from)?;
            zip.write_all(&data).map_err(anyhow::Error::from)?;
        }
        let bytes = zip.finish().map_err(anyhow::Error::from)?.into_inner();
        ("matches.zip".into(), bytes, "application/zip")
    };
    let disposition = format!(
        "attachment; filename=\"{}\"",
        name.chars()
            .map(|c| if c.is_ascii() { c } else { '_' })
            .collect::<String>()
    );
    Ok((
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    )
        .into_response())
}

#[derive(OpenApi)]
#[openapi(paths(list_databases,create_database,update_database,delete_database,clear_database,library_matches,get_metadata,update_metadata,actions,export),components(schemas(Database,DatabaseWrite,DatabaseMatch,LibraryOccurrence,LibraryPage,MetadataWrite,LibraryAction,ActionItem,ActionResponse,ExportRequest)),tags((name="Databases",description="Independent match databases and library management")))]
pub struct DatabaseDoc;
