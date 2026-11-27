use super::*;
use crate::domain_error::DomainError;
use std::path::{Path as FsPath, PathBuf};

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

pub(crate) async fn list_databases_service(pool: &PgPool) -> Result<Vec<Database>, DomainError> {
    Ok(list_library_databases(pool).await?)
}

pub(crate) async fn create_database_service(
    pool: &PgPool,
    w: DatabaseWrite,
) -> Result<Database, DomainError> {
    validate_database(&w)?;
    Ok(insert_database(pool, w.name.trim(), &w.description).await?)
}

pub(crate) async fn update_database_service(
    pool: &PgPool,
    id: Uuid,
    w: DatabaseWrite,
) -> Result<Database, DomainError> {
    validate_database(&w)?;
    let version = w
        .version
        .filter(|v| *v > 0)
        .ok_or_else(|| DomainError::invalid("version is required"))?;
    Ok(write_database(pool, id, w.name.trim(), &w.description, version).await?)
}

pub(crate) async fn delete_database_service(
    pool: &PgPool,
    id: Uuid,
    version: i64,
    delete_matches: bool,
) -> Result<(), DomainError> {
    remove_database_content(
        pool,
        id,
        version,
        DatabaseRemoval::Delete { delete_matches },
    )
    .await?;
    Ok(())
}

pub(crate) async fn clear_database_service(
    pool: &PgPool,
    id: Uuid,
    version: i64,
) -> Result<(), DomainError> {
    remove_database_content(pool, id, version, DatabaseRemoval::Clear).await?;
    Ok(())
}

pub(crate) async fn get_metadata_service(
    pool: &PgPool,
    id: Uuid,
) -> Result<DatabaseMatch, DomainError> {
    Ok(database_match(pool, id).await?)
}

pub(crate) async fn update_metadata_service(
    state: &AppState,
    id: Uuid,
    w: MetadataWrite,
) -> Result<DatabaseMatch, DomainError> {
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
        return Err(DomainError::invalid("Invalid match metadata"));
    }
    write_match_metadata(&state.pool, id, w).await?;
    state.browse_wake.notify_one();
    Ok(database_match(&state.pool, id).await?)
}

pub(crate) async fn apply_actions_service(
    state: &AppState,
    mut a: LibraryAction,
) -> Result<ActionResponse, DomainError> {
    if a.match_ids.is_empty() || a.match_ids.len() > MAX_ACTIONS {
        return Err(DomainError::invalid("Select between 1 and 100 matches"));
    }
    if !["copy", "move", "delete", "tags"].contains(&a.action.as_str()) {
        return Err(DomainError::invalid("Unknown library action"));
    }
    if ["copy", "move"].contains(&a.action.as_str()) {
        require_library_destination(&state.pool, a.required_destination()?).await?;
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
        results.push(action_one(&state.pool, &a, id).await?)
    }
    state.browse_wake.notify_one();
    Ok(ActionResponse { results })
}

struct ExportArtifact {
    filename: String,
    content_type: &'static str,
    bytes: Vec<u8>,
}

async fn build_export_service(
    pool: &PgPool,
    r: ExportRequest,
) -> Result<ExportArtifact, DomainError> {
    if r.match_ids.is_empty() || r.match_ids.len() > MAX_EXPORTS {
        return Err(DomainError::invalid("Select between 1 and 50 matches"));
    }
    let mut seen = HashSet::new();
    if r.match_ids.iter().any(|id| !seen.insert(*id)) {
        return Err(DomainError::invalid("Duplicate match IDs are not allowed"));
    }
    let files = match load_export_sources(pool, &r.match_ids, r.format).await? {
        ExportSources::Files(files) => files,
        ExportSources::Xg(sources) => {
            // The conversion replays every checker play, so it runs off the async
            // workers after the shared row locks are released.
            tokio::task::spawn_blocking(move || {
                sources
                    .into_iter()
                    .map(|source| {
                        let meta = crate::import::XgExportMetadata {
                            match_id: source.id,
                            played_on: source.played_on,
                            event: source.tournament.as_deref(),
                            round: source.round.as_deref(),
                            place: source.place.as_deref(),
                        };
                        match crate::import::sgf_to_xg(&source.sgf, &meta) {
                            Ok(bytes) => Ok((source.id, source.filename, bytes)),
                            Err(error) => Err(ApiError::invalid(format!(
                                "{} cannot be exported as XG: {error:#}",
                                source.filename
                            ))),
                        }
                    })
                    .collect::<Result<Vec<_>, ApiError>>()
            })
            .await
            .map_err(anyhow::Error::from)??
        }
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
        if let Some(extension) = r.format.converted_extension() {
            let stem = clean.rsplit_once('.').map(|(s, _)| s).unwrap_or(&clean);
            format!("{stem}-{id}.{extension}")
        } else {
            format!("{id}-{clean}")
        }
    };
    let mut files = files.into_iter();
    let first = files.next().ok_or_else(DomainError::missing)?;
    let (filename, bytes, content_type) = if files.len() == 0 {
        let (id, name, data) = first;
        (safe_name(id, &name), data, "application/octet-stream")
    } else {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (id, name, data) in std::iter::once(first).chain(files) {
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
    Ok(ExportArtifact {
        filename,
        content_type,
        bytes,
    })
}

pub(crate) async fn export_to_path_service(
    pool: &PgPool,
    r: ExportRequest,
    destination: &FsPath,
) -> Result<PathBuf, DomainError> {
    let artifact = build_export_service(pool, r).await?;
    let target = if tokio::fs::metadata(destination)
        .await
        .is_ok_and(|metadata| metadata.is_dir())
    {
        destination.join(&artifact.filename)
    } else {
        destination.to_path_buf()
    };
    tokio::fs::write(&target, &artifact.bytes)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(target)
}

#[utoipa::path(get,path="/api/databases",responses((status=200,body=[Database])),tag="Databases")]
pub(super) async fn list_databases(State(s): State<AppState>) -> ApiResult<Json<Vec<Database>>> {
    Ok(Json(list_databases_service(&s.pool).await?))
}

#[utoipa::path(post,path="/api/databases",request_body=DatabaseWrite,responses((status=201,body=Database),(status=422,body=ErrorBody)),tag="Databases")]
pub(super) async fn create_database(
    State(s): State<AppState>,
    Json(w): Json<DatabaseWrite>,
) -> ApiResult<(StatusCode, Json<Database>)> {
    Ok((
        StatusCode::CREATED,
        Json(create_database_service(&s.pool, w).await?),
    ))
}

#[utoipa::path(put,path="/api/databases/{id}",params(("id"=Uuid,Path)),request_body=DatabaseWrite,responses((status=200,body=Database),(status=409,body=ErrorBody)),tag="Databases")]
pub(super) async fn update_database(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(w): Json<DatabaseWrite>,
) -> ApiResult<Json<Database>> {
    Ok(Json(update_database_service(&s.pool, id, w).await?))
}

#[utoipa::path(delete,path="/api/databases/{id}",params(("id"=Uuid,Path),DatabaseDelete),responses((status=204),(status=409,body=ErrorBody)),tag="Databases")]
pub(super) async fn delete_database(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<DatabaseDelete>,
) -> ApiResult<StatusCode> {
    delete_database_service(&s.pool, id, q.version, q.delete_matches).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete,path="/api/databases/{id}/matches",params(("id"=Uuid,Path),DatabaseVersion),responses((status=204),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Databases")]
pub(super) async fn clear_database(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<DatabaseVersion>,
) -> ApiResult<StatusCode> {
    clear_database_service(&s.pool, id, q.version).await?;
    Ok(StatusCode::NO_CONTENT)
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
    Ok(Json(get_metadata_service(&s.pool, id).await?))
}

#[utoipa::path(put,path="/api/matches/{id}/metadata",params(("id"=Uuid,Path)),request_body=MetadataWrite,responses((status=200,body=DatabaseMatch),(status=409,body=ErrorBody)),tag="Databases")]
pub(super) async fn update_metadata(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(w): Json<MetadataWrite>,
) -> ApiResult<Json<DatabaseMatch>> {
    Ok(Json(update_metadata_service(&s, id, w).await?))
}

#[utoipa::path(post,path="/api/library/actions",request_body=LibraryAction,responses((status=200,body=ActionResponse),(status=409,body=ErrorBody,description="Target is reserved for paused sessions"),(status=422,body=ErrorBody)),tag="Databases")]
pub(super) async fn actions(
    State(s): State<AppState>,
    Json(a): Json<LibraryAction>,
) -> ApiResult<Json<ActionResponse>> {
    Ok(Json(apply_actions_service(&s, a).await?))
}

#[utoipa::path(post,path="/api/library/export",request_body=ExportRequest,responses((status=200,description="Original, SGF or XG file, or a ZIP for multiple matches",content_type="application/octet-stream"),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Databases")]
pub(super) async fn export(
    State(s): State<AppState>,
    Json(r): Json<ExportRequest>,
) -> ApiResult<Response> {
    let artifact = build_export_service(&s.pool, r).await?;
    let disposition = format!(
        "attachment; filename=\"{}\"",
        artifact
            .filename
            .chars()
            .map(|c| if c.is_ascii() { c } else { '_' })
            .collect::<String>()
    );
    Ok((
        [
            (header::CONTENT_TYPE, artifact.content_type.to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        artifact.bytes,
    )
        .into_response())
}

#[derive(OpenApi)]
#[openapi(paths(list_databases,create_database,update_database,delete_database,clear_database,library_matches,get_metadata,update_metadata,actions,export),components(schemas(Database,DatabaseWrite,DatabaseMatch,LibraryOccurrence,LibraryPage,MetadataWrite,LibraryAction,ActionItem,ActionResponse,ExportRequest,ExportFormat)),tags((name="Databases",description="Independent match databases and library management")))]
pub struct DatabaseDoc;
