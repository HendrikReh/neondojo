use super::*;

pub(super) const SAVED_SELECT: &str = "SELECT f.*,coalesce((f.definition->>'database_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM databases d WHERE d.id=(f.definition->>'database_id')::uuid),false) AS database_missing FROM position_filters f";

pub(super) fn saved(row: sqlx::postgres::PgRow) -> ApiResult<SavedPositionFilter> {
    Ok(SavedPositionFilter {
        id: row.get("id"),
        name: row.get("name"),
        definition: serde_json::from_value(row.get("definition")).map_err(anyhow::Error::from)?,
        version: row.get("version"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        database_missing: row.get("database_missing"),
    })
}

pub(super) fn validate_name(name: String) -> ApiResult<String> {
    let name = name.trim().to_owned();
    if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(ApiError::invalid(
            "Filter name must be 1–120 characters without control characters",
        ));
    }
    Ok(name)
}

pub(super) fn write_error(e: sqlx::Error) -> ApiError {
    if e.as_database_error()
        .is_some_and(|e| e.is_unique_violation())
    {
        conflict("A position filter with this name already exists")
    } else {
        anyhow::Error::from(e).into()
    }
}

#[utoipa::path(get,path="/api/position-filters",responses((status=200,body=[SavedPositionFilter])),tag="Positions")]
pub(super) async fn list_filters(
    State(s): State<AppState>,
) -> ApiResult<Json<Vec<SavedPositionFilter>>> {
    let rows = sqlx::query(&format!("{SAVED_SELECT} ORDER BY lower(f.name),f.id"))
        .fetch_all(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(Json(rows.into_iter().map(saved).collect::<ApiResult<_>>()?))
}

#[utoipa::path(post,path="/api/position-filters",request_body=CreatePositionFilter,responses((status=201,body=SavedPositionFilter),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Positions")]
pub(super) async fn create_filter(
    State(s): State<AppState>,
    value: Result<Json<CreatePositionFilter>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<SavedPositionFilter>)> {
    let input = json(value)?;
    let name = validate_name(input.name)?;
    let definition = input.definition.validate()?;
    database_exists(&s.pool, definition.database_id).await?;
    let mut tx = s.pool.begin().await.map_err(anyhow::Error::from)?;
    crate::advisory_locks::xact_lock(&mut tx, crate::advisory_locks::POSITION_FILTER_LIMIT)
        .await
        .map_err(anyhow::Error::from)?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM position_filters")
        .fetch_one(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    if count >= 200 {
        return Err(ApiError::invalid(
            "At most 200 saved position filters are supported",
        ));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO position_filters(id,name,definition) VALUES($1,$2,$3)")
        .bind(id)
        .bind(name)
        .bind(serde_json::to_value(definition).map_err(anyhow::Error::from)?)
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
    let result = saved(
        sqlx::query(&format!("{SAVED_SELECT} WHERE f.id=$1"))
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?,
    )?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(put,path="/api/position-filters/{id}",params(("id"=Uuid,Path)),request_body=UpdatePositionFilter,responses((status=200,body=SavedPositionFilter),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Positions")]
pub(super) async fn update_filter(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    value: Result<Json<UpdatePositionFilter>, JsonRejection>,
) -> ApiResult<Json<SavedPositionFilter>> {
    let input = json(value)?;
    let name = validate_name(input.name)?;
    let definition = input.definition.validate()?;
    if input.expected_version < 1 {
        return Err(ApiError::invalid("expected_version must be positive"));
    }
    database_exists(&s.pool, definition.database_id).await?;
    let mut tx = s.pool.begin().await.map_err(anyhow::Error::from)?;
    let changed=sqlx::query("UPDATE position_filters SET name=$2,definition=$3,version=version+1,updated_at=now() WHERE id=$1 AND version=$4").bind(id).bind(name).bind(serde_json::to_value(definition).map_err(anyhow::Error::from)?).bind(input.expected_version).execute(&mut *tx).await.map_err(write_error)?.rows_affected();
    let row = sqlx::query(&format!("{SAVED_SELECT} WHERE f.id=$1"))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if changed == 0 {
        return Err(conflict(
            "Position filter was changed; reload before saving",
        ));
    }
    let result = saved(row)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(result))
}

#[utoipa::path(delete,path="/api/position-filters/{id}",params(("id"=Uuid,Path),DeletePositionFilter),responses((status=204),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Positions")]
pub(super) async fn delete_filter(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    value: Result<Query<DeletePositionFilter>, QueryRejection>,
) -> ApiResult<StatusCode> {
    let input = query(value)?;
    if input.expected_version < 1 {
        return Err(ApiError::invalid("expected_version must be positive"));
    }
    let changed = sqlx::query("DELETE FROM position_filters WHERE id=$1 AND version=$2")
        .bind(id)
        .bind(input.expected_version)
        .execute(&s.pool)
        .await
        .map_err(anyhow::Error::from)?
        .rows_affected();
    if changed == 0 {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM position_filters WHERE id=$1)")
                .bind(id)
                .fetch_one(&s.pool)
                .await
                .map_err(anyhow::Error::from)?;
        return Err(if exists {
            conflict("Position filter was changed; reload before deleting")
        } else {
            ApiError::missing()
        });
    }
    Ok(StatusCode::NO_CONTENT)
}
