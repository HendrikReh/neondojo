//! Support for the ignored large-library performance test. It explains the
//! statements of a decision search with the parameters the handler binds, so
//! the measured plans are the production ones.
use super::{imported, index, search};
use crate::api::{ApiError, ApiResult};
use axum::http::Uri;
use serde_json::Value;
use sqlx::PgPool;

const EXPLAIN: &str = "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ";

/// Store the current fingerprint for every match that already has a
/// decision-index entry, without loading its decisions. This marks synthetic
/// decision rows as current, so a search finds nothing to rebuild.
pub async fn adopt_current_fingerprints(pool: &PgPool) -> ApiResult<u64> {
    Ok(index::adopt_current_fingerprints(pool)
        .await
        .map_err(anyhow::Error::from)?)
}

/// The plans of a warm `GET /api/study/decisions?{query}`: the legacy-catalog
/// check, the freshness check, the total and the page, in that order.
pub async fn decision_search(pool: &PgPool, query: &str) -> ApiResult<Vec<(&'static str, Value)>> {
    let uri: Uri = format!("/api/study/decisions?{query}")
        .parse()
        .map_err(|_| ApiError::invalid("Invalid query"))?;
    #[expect(
        clippy::disallowed_types,
        reason = "there is no request to extract from"
    )]
    let axum::extract::Query(query) =
        axum::extract::Query::try_from_uri(&uri).map_err(|e| ApiError::invalid(e.body_text()))?;
    let search = search::Search::new(pool, query).await?;
    let player = search.player().to_vec();
    let mut plans = Vec::new();
    for (name, sql) in [
        ("legacy_size", imported::legacy_size_sql()),
        ("legacy_ids", imported::legacy_ids_sql()),
    ] {
        let plan = sqlx::query_scalar(&format!("{EXPLAIN}{sql}"))
            .bind(search.database())
            .bind(None::<uuid::Uuid>)
            .bind(Some(player.clone()))
            .fetch_one(pool)
            .await
            .map_err(anyhow::Error::from)?;
        plans.push((name, plan));
    }
    let plan = sqlx::query_scalar(&format!("{EXPLAIN}{}", index::scope_candidates_sql()))
        .bind(index::version())
        .bind(search.database())
        .bind(&player)
        .fetch_one(pool)
        .await
        .map_err(anyhow::Error::from)?;
    plans.push(("candidates", plan));
    let mut tx = super::source::read_snapshot(pool).await?;
    let count = format!("{EXPLAIN}{}", search.count_sql());
    let plan = search
        .bind(sqlx::query_scalar(&count))
        .fetch_one(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    plans.push(("count", plan));
    let page = format!("{EXPLAIN}{}", search.page_sql());
    let plan = search
        .bind_page(sqlx::query_scalar(&page))?
        .fetch_one(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    plans.push(("page", plan));
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(plans)
}
