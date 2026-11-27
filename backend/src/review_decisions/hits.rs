use super::*;
use std::collections::BTreeSet;
use std::sync::Mutex;

const MAX_QUEUED_HIT_SETS: usize = 128;

/// Bounded in-memory queue of `(match_id, analysis_id)` review hit sets that
/// read-only GET requests found missing or stale. `browse_indexes::worker`
/// drains and builds them outside HTTP GET transactions.
#[derive(Default)]
pub(crate) struct HitRefreshQueue {
    pending: Mutex<BTreeSet<(Uuid, String)>>,
}

impl HitRefreshQueue {
    pub(crate) fn enqueue(&self, match_id: Uuid, analysis_id: &str) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pending.len() < MAX_QUEUED_HIT_SETS {
            pending.insert((match_id, analysis_id.to_owned()));
        }
    }

    pub(crate) fn take_batch(&self, limit: usize) -> Vec<(Uuid, String)> {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut batch = Vec::with_capacity(limit.min(pending.len()));
        for _ in 0..limit {
            let Some(item) = pending.pop_first() else {
                break;
            };
            batch.push(item);
        }
        batch
    }
}

/// Saved data a hit set was built from: projection version, analysis, source
/// revision and catalog state, 0089 analysis revision tokens, and per-game
/// cube rules.
pub(super) async fn hits_fingerprint(
    connection: &mut sqlx::PgConnection,
    match_id: Uuid,
    analysis_id: &str,
    job_id: Option<Uuid>,
) -> ApiResult<String> {
    Ok(sqlx::query_scalar(
        "SELECT concat_ws('|', $3::text, m.source_revision::text, m.format,
                m.moves_cataloged::text, m.imported_analysis_loaded::text,
                coalesce(n.revision, '0:0'),
                coalesce(i.revision, ''),
                coalesce((SELECT string_agg(
                    g.game_no::text || ':' || coalesce(g.cube_enabled::text, 'null'),
                    ',' ORDER BY g.game_no
                ) FROM match_games g WHERE g.match_uuid = m.id), ''))
         FROM matches m
         LEFT JOIN native_analysis_revisions n ON n.job_id = $2
         LEFT JOIN imported_analysis_revisions i ON i.match_id = m.id
         WHERE m.id = $1",
    )
    .bind(match_id)
    .bind(job_id)
    .bind(format!("{HITS_VERSION}:{analysis_id}"))
    .fetch_one(connection)
    .await
    .map_err(anyhow::Error::from)?)
}

/// Refresh one `(match_id, analysis_id)` review hit set outside a GET request.
/// Returns `Ok(false)` when the match or analysis no longer exists or is uncataloged.
pub async fn refresh_hits(
    pool: &sqlx::PgPool,
    match_id: Uuid,
    analysis_id: &str,
) -> ApiResult<bool> {
    let mut tx = crate::database_pool::begin_read_snapshot(pool).await?;
    let row: Option<(MatchFormat, bool, bool)> = sqlx::query_as(
        "SELECT format,moves_cataloged,imported_analysis_loaded FROM matches WHERE id=$1",
    )
    .bind(match_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let Some((format, moves_cataloged, imported_loaded)) = row else {
        return Ok(false);
    };
    if !moves_cataloged || !imported_loaded {
        return Ok(false);
    }
    let selected =
        match crate::analysis_catalog::resolve_selection_on(&mut tx, match_id, Some(analysis_id))
            .await
        {
            Ok(Some(selected)) => selected,
            Ok(None) | Err(_) => return Ok(false),
        };
    tx.commit().await.map_err(anyhow::Error::from)?;
    let (job, imported) = match selected {
        crate::analysis_catalog::SelectedAnalysis::Job(job) => {
            if job.kind == "analysis" {
                return Ok(false);
            }
            (Some(job), false)
        }
        crate::analysis_catalog::SelectedAnalysis::Imported { .. } => (None, true),
    };
    build_hits(
        pool,
        match_id,
        analysis_id,
        job.as_deref(),
        imported,
        format,
    )
    .await?;
    Ok(true)
}

pub(crate) async fn drain_hit_queue(state: &AppState) -> ApiResult<bool> {
    let batch = state.review_hit_queue.take_batch(8);
    if batch.is_empty() {
        return Ok(false);
    }
    for (match_id, analysis_id) in &batch {
        if let Err(error) = refresh_hits(&state.pool, *match_id, analysis_id).await {
            tracing::warn!(%match_id, %analysis_id, error = %error.1.error, "Failed to refresh review hit set");
        }
    }
    Ok(batch.len() == 8)
}

/// One validated decision of a hit set, in review order.
pub(super) struct Hit {
    pub(super) game_no: i32,
    pub(super) decision_no: i32,
    pub(super) checker: bool,
    pub(super) id: String,
    pub(super) player: i32,
    pub(super) evaluated: bool,
    pub(super) equity_loss: f64,
}

/// Project every decision of the analysis once and store the valid ones with
/// what numbered pages need to count and select them.
pub(super) async fn build_hits(
    pool: &sqlx::PgPool,
    match_id: Uuid,
    analysis_id: &str,
    job: Option<&Job>,
    imported: bool,
    format: MatchFormat,
) -> ApiResult<()> {
    let job_id = job.map(|job| job.id);
    let mut tx = pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ")
        .await
        .map_err(anyhow::Error::from)?;
    // Concurrent builds for the same analysis must not interleave their rows.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, $2))")
        .bind(format!("{match_id}:{analysis_id}"))
        .bind(crate::advisory_locks::REVIEW_HIT_SET_SEED)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let fingerprint = hits_fingerprint(&mut tx, match_id, analysis_id, job_id).await?;
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT fingerprint FROM review_hit_sets WHERE match_id=$1 AND analysis_id=$2",
    )
    .bind(match_id)
    .bind(analysis_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if stored.as_deref() == Some(fingerprint.as_str()) {
        tx.commit().await.map_err(anyhow::Error::from)?;
        return Ok(());
    }
    let mut hits = Vec::new();
    let mut cursor: Option<(i32, i32, bool, String)> = None;
    loop {
        let rows = review_rows_query()
            .bind(match_id)
            .bind(job_id)
            .bind(None::<i32>)
            .bind(None::<i32>)
            .bind(None::<String>)
            .bind(cursor.as_ref().map(|c| c.0))
            .bind(cursor.as_ref().map(|c| c.1))
            .bind(cursor.as_ref().map(|c| c.2))
            .bind(cursor.as_ref().map(|c| c.3.clone()))
            .bind(SCAN_LIMIT)
            .bind(imported)
            .bind(None::<Vec<String>>)
            .fetch_all(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
        let complete = rows.len() < SCAN_LIMIT as usize;
        cursor = rows.last().map(|row| {
            (
                row.game_no,
                row.decision_no,
                row.kind == "checker",
                row.id.clone(),
            )
        });
        let job = job.cloned();
        let batch = tokio::task::spawn_blocking(move || {
            rows.into_iter()
                .filter_map(|row| {
                    let (game_no, decision_no, checker, id, player) = (
                        row.game_no,
                        row.decision_no,
                        row.kind == "checker",
                        row.id.clone(),
                        row.player,
                    );
                    let decision = project(row, match_id, format, job.as_ref())?.decision;
                    Some(Hit {
                        game_no,
                        decision_no,
                        checker,
                        id,
                        player,
                        evaluated: decision.evaluation_status == "evaluated",
                        equity_loss: decision.equity_loss,
                    })
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(anyhow::Error::from)?;
        hits.extend(batch);
        if complete {
            break;
        }
    }
    sqlx::query("DELETE FROM review_hit_sets WHERE match_id=$1 AND analysis_id=$2")
        .bind(match_id)
        .bind(analysis_id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    sqlx::query("INSERT INTO review_hit_sets(match_id,analysis_id,fingerprint) VALUES($1,$2,$3)")
        .bind(match_id)
        .bind(analysis_id)
        .bind(&fingerprint)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    sqlx::query(
        "INSERT INTO review_hits(match_id,analysis_id,game_no,decision_no,checker,id,player,evaluated,equity_loss)
         SELECT $1,$2,u.game_no,u.decision_no,u.checker,u.id,u.player,u.evaluated,u.equity_loss
         FROM unnest($3::int[],$4::int[],$5::bool[],$6::text[],$7::int[],$8::bool[],$9::float8[])
              AS u(game_no,decision_no,checker,id,player,evaluated,equity_loss)",
    )
    .bind(match_id)
    .bind(analysis_id)
    .bind(hits.iter().map(|h| h.game_no).collect::<Vec<_>>())
    .bind(hits.iter().map(|h| h.decision_no).collect::<Vec<_>>())
    .bind(hits.iter().map(|h| h.checker).collect::<Vec<_>>())
    .bind(hits.iter().map(|h| h.id.clone()).collect::<Vec<_>>())
    .bind(hits.iter().map(|h| h.player).collect::<Vec<_>>())
    .bind(hits.iter().map(|h| h.evaluated).collect::<Vec<_>>())
    .bind(hits.iter().map(|h| h.equity_loss).collect::<Vec<_>>())
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(())
}

/// A numbered page answered from a current hit set.
pub(super) struct HitPage<'a> {
    pub(super) match_id: Uuid,
    pub(super) analysis_id: &'a str,
    pub(super) job_id: Option<Uuid>,
    pub(super) imported: bool,
    pub(super) format: MatchFormat,
    pub(super) job: Option<&'a Job>,
    pub(super) game: Option<i32>,
    pub(super) query: &'a ReviewQuery,
    pub(super) thresholds: ErrorThresholds,
    pub(super) offset: i64,
    pub(super) limit: i64,
}

impl HitPage<'_> {
    /// None when the page's decisions no longer project as the set recorded;
    /// the caller then scans the analysis instead.
    pub(super) async fn load(
        self,
        connection: &mut sqlx::PgConnection,
    ) -> ApiResult<Option<ReviewDecisionPage>> {
        #[derive(FromRow)]
        struct Selected {
            id: String,
            evaluated: bool,
            equity_loss: f64,
        }
        let rows: Vec<Selected> = sqlx::query_as(
            "SELECT id,evaluated,equity_loss FROM review_hits
             WHERE match_id=$1 AND analysis_id=$2
               AND ($3::integer IS NULL OR game_no=$3) AND ($4::integer IS NULL OR player=$4)
               AND ($5::text IS NULL OR checker=($5='checker'))
             ORDER BY game_no,decision_no,checker,id",
        )
        .bind(self.match_id)
        .bind(self.analysis_id)
        .bind(self.game)
        .bind(self.query.player)
        .bind(&self.query.kind)
        .fetch_all(&mut *connection)
        .await
        .map_err(anyhow::Error::from)?;
        let selected: Vec<String> = rows
            .into_iter()
            .filter(|row| {
                severity_selected(
                    review_severity(row.evaluated, row.equity_loss, self.thresholds),
                    self.query.errors_only,
                    self.query.severity,
                )
            })
            .map(|row| row.id)
            .collect();
        let total = selected.len() as i64;
        let page: Vec<String> = selected
            .into_iter()
            .skip(usize::try_from(self.offset).map_err(anyhow::Error::from)?)
            .take(usize::try_from(self.limit).map_err(anyhow::Error::from)?)
            .collect();
        let mut items = Vec::new();
        if !page.is_empty() {
            let rows = review_rows_query()
                .bind(self.match_id)
                .bind(self.job_id)
                .bind(self.game)
                .bind(self.query.player)
                .bind(&self.query.kind)
                .bind(None::<i32>)
                .bind(None::<i32>)
                .bind(None::<bool>)
                .bind(None::<String>)
                .bind(page.len() as i64 + 1)
                .bind(self.imported)
                .bind(&page)
                .fetch_all(&mut *connection)
                .await
                .map_err(anyhow::Error::from)?;
            let (match_id, format, job) = (self.match_id, self.format, self.job.cloned());
            let (errors_only, severity, thresholds) =
                (self.query.errors_only, self.query.severity, self.thresholds);
            items = tokio::task::spawn_blocking(move || {
                rows.into_iter()
                    .filter_map(|row| {
                        filtered_decision(
                            row,
                            match_id,
                            format,
                            job.as_ref(),
                            errors_only,
                            severity,
                            thresholds,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .map_err(anyhow::Error::from)?;
            if !items.iter().map(|item| &item.id).eq(page.iter()) {
                return Ok(None);
            }
        }
        Ok(Some(ReviewDecisionPage {
            items,
            next_cursor: None,
            total: Some(total),
        }))
    }
}
