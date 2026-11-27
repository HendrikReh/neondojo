use crate::{
    api::{ApiError, ApiResult},
    model::Job,
};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug)]
pub(crate) enum SelectedAnalysis {
    Job(Box<Job>),
    Imported { match_id: Uuid },
}
impl SelectedAnalysis {
    pub(crate) fn id(&self) -> String {
        match self {
            Self::Job(job) => format!("job:{}", job.id),
            Self::Imported { match_id } => format!("imported:{match_id}"),
        }
    }
}

pub(crate) async fn resolve_selection(
    pool: &PgPool,
    match_id: Uuid,
    requested: Option<&str>,
) -> ApiResult<Option<SelectedAnalysis>> {
    crate::play::require_review_available(pool, match_id).await?;
    if !crate::imported_analysis::ensure_catalog(pool, match_id).await? {
        return Err(ApiError::missing());
    }
    let mut tx = crate::database_pool::begin_read_snapshot(pool).await?;
    let result = resolve_selection_on(&mut tx, match_id, requested).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(result)
}

/// Resolve in the caller's snapshot without acquiring a second pool connection.
/// Callers must load the lazy imported catalog before opening read-only snapshots.
pub(crate) async fn resolve_selection_on(
    connection: &mut PgConnection,
    match_id: Uuid,
    requested: Option<&str>,
) -> ApiResult<Option<SelectedAnalysis>> {
    crate::play::require_review_available_on(&mut *connection, match_id).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM matches WHERE id=$1)")
        .bind(match_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(anyhow::Error::from)?;
    if !exists {
        return Err(ApiError::missing());
    }
    let selected: Option<String> = if let Some(requested) = requested {
        Some(requested.to_owned())
    } else {
        let stored:Option<(String,bool)>=sqlx::query_as("SELECT s.analysis_id,s.source_revision=m.source_revision FROM analysis_selections s JOIN matches m ON m.id=s.match_id WHERE s.match_id=$1")
            .bind(match_id).fetch_optional(&mut *connection).await.map_err(anyhow::Error::from)?;
        match stored {
            Some((_, false)) => {
                return Err(ApiError::invalid(
                    "Selected analysis belongs to an older source revision",
                ));
            }
            Some((key, true)) => Some(key),
            None => None,
        }
    };
    let row: Option<(String, Option<Uuid>)> = if let Some(key) = selected {
        let row=sqlx::query_as("SELECT source,job_id FROM analysis_catalog_sources WHERE match_id=$1 AND analysis_id=$2")
            .bind(match_id).bind(key).fetch_optional(&mut *connection).await.map_err(anyhow::Error::from)?;
        if row.is_none() {
            return Err(ApiError::invalid(
                "Selected analysis is unavailable or belongs to another source revision or match",
            ));
        }
        row
    } else {
        sqlx::query_as("SELECT source,job_id FROM effective_analysis_selections WHERE match_id=$1")
            .bind(match_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(anyhow::Error::from)?
    };
    match row {
        Some((source, None)) if source == "imported" => {
            Ok(Some(SelectedAnalysis::Imported { match_id }))
        }
        Some((_, Some(id))) => {
            let job = sqlx::query_as("SELECT * FROM jobs WHERE id=$1")
                .bind(id)
                .fetch_one(connection)
                .await
                .map_err(anyhow::Error::from)?;
            Ok(Some(SelectedAnalysis::Job(Box::new(job))))
        }
        _ => Ok(None),
    }
}
