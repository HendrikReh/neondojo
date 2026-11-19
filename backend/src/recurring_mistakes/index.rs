//! Persisted per-match index of the decisions in the displayed analysis.
//!
//! Mistake groups and the decision search read this index with SQL. A match
//! is re-indexed when its fingerprint changes: another analysis is displayed,
//! its saved decisions were added or removed, the source or a player name
//! changed, or the index format moved on. Updating a saved decision drops the
//! match's entry instead (migration 0080).
use super::{
    can_practice_correct_cube, kind, opportunity,
    search::{DecisionHit, context, owner_filter},
    source::{SourceDecision, load},
};
use crate::api::{ApiError, ApiResult};
use chrono::NaiveDate;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// Bump when the stored columns or the hit format change.
const INDEX_VERSION: &str = "1";
/// Matches re-indexed by one request; a larger backlog asks for a retry.
const MAX_REBUILDS: usize = 400;
/// Loads of one match per request while its saved decisions keep changing.
const MAX_PUBLISH_ATTEMPTS: usize = 3;
const CHANGED: &str =
    "Saved analyses changed while the decision index was updated; retry this request";

#[derive(Clone, FromRow)]
struct Candidate {
    id: Uuid,
    fingerprint: String,
    played: NaiveDate,
    stored: Option<String>,
}

/// Each match's current fingerprint and stored index entry. `$1` is the index
/// version; callers append their filter. The fingerprint covers the displayed
/// analysis, the source, the player names and the format. Inserted or deleted
/// decisions change the count or the sum of their never-reused ids, read from
/// move_analyses_page without the analysis JSON. An UPDATE drops the affected
/// matches' index entries in a trigger (migration 0080).
const CANDIDATES: &str = "SELECT m.id,
                concat_ws('|', $1::text, coalesce(sel.analysis_id,''), coalesce(sel.source,''),
                          coalesce(sel.job_id::text,''), m.source_revision::text,
                          lower(btrim(m.white)), lower(btrim(m.black)),
                          m.imported_analysis_loaded::text, m.moves_cataloged::text,
                          live.blocked::text,
                          (SELECT count(*)::text||':'||coalesce(sum(a.id),0)::text
                           FROM move_analyses a WHERE a.job_id=sel.job_id)) AS fingerprint,
                coalesce(m.played_on, m.created_at::date) AS played,
                x.fingerprint AS stored
         FROM matches m
         -- OFFSET 0 keeps the catalog lookup scoped to this match.
         LEFT JOIN LATERAL (
             SELECT s.analysis_id,s.source,s.job_id FROM effective_analysis_selections s
             WHERE s.match_id=m.id OFFSET 0
         ) sel ON true
         CROSS JOIN LATERAL (
             SELECT EXISTS(
                 SELECT 1 FROM play_sessions live
                 WHERE live.library_match_id=m.id
                   AND coalesce(live.state->>'play_mode','competition')='competition'
                   AND live.state->>'phase'<>'match_over'
                   AND NOT (live.state->>'match_length'='0' AND live.state->>'phase'='game_over')
             ) AS blocked
         ) live
         LEFT JOIN decision_index_matches x ON x.match_id=m.id
         WHERE NOT m.collection_only";

fn version() -> String {
    format!("{INDEX_VERSION}:{}", crate::game_plan::CLASSIFIER)
}

/// Bring the index up to date for one player's matches in the scope.
pub(super) async fn ensure(
    pool: &PgPool,
    database: Option<Uuid>,
    player: &[String],
) -> ApiResult<()> {
    // Older imports gain their displayed analysis only once they are cataloged.
    // This is bounded per request and reports when a retry is needed.
    super::imported::catalog_legacy(pool, database, None, Some(player)).await?;
    let candidates: Vec<Candidate> = sqlx::query_as(&format!(
        "{CANDIDATES} AND ($2::uuid IS NULL OR m.database_id=$2)
           AND (lower(btrim(m.white))=ANY($3) OR lower(btrim(m.black))=ANY($3))"
    ))
    .bind(version())
    .bind(database)
    .bind(player)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    let stale: Vec<Candidate> = candidates
        .into_iter()
        .filter(|candidate| candidate.stored.as_deref() != Some(candidate.fingerprint.as_str()))
        .collect();
    if stale.len() > MAX_REBUILDS {
        return Err(ApiError::invalid(super::source::TOO_BROAD));
    }
    for candidate in stale {
        publish(pool, candidate).await?;
    }
    Ok(())
}

/// What `store` found under the match lock.
enum Published {
    /// The index holds the match's current state, or the match left its scope.
    Current,
    /// The decisions changed after they were loaded; load them again.
    Outdated(Candidate),
}

/// Re-index one match until the stored entry matches its saved decisions.
async fn publish(pool: &PgPool, mut candidate: Candidate) -> ApiResult<()> {
    for _ in 0..MAX_PUBLISH_ATTEMPTS {
        let rows = load(pool, None, Some(candidate.id), None).await?;
        match store(pool, &candidate, rows).await? {
            Published::Current => return Ok(()),
            Published::Outdated(current) => candidate = current,
        }
    }
    Err(ApiError::invalid(CHANGED))
}

async fn store(
    pool: &PgPool,
    candidate: &Candidate,
    rows: Vec<SourceDecision>,
) -> ApiResult<Published> {
    let mut seq = Vec::new();
    let mut game_no = Vec::new();
    let mut decision_no = Vec::new();
    let mut player_name = Vec::new();
    let mut question_kind = Vec::new();
    let mut game_plan = Vec::new();
    let mut opportunities: Vec<Option<&'static str>> = Vec::new();
    let mut mistake_kind = Vec::new();
    let mut equity_loss = Vec::new();
    let mut match_length = Vec::new();
    let mut away_own: Vec<Option<i32>> = Vec::new();
    let mut away_opponent: Vec<Option<i32>> = Vec::new();
    let mut cube_value = Vec::new();
    let mut cube_owner = Vec::new();
    let mut hits = Vec::new();
    for row in rows {
        let Some(context) = context(&row) else {
            continue;
        };
        let Some(name) = usize::try_from(row.decision.player)
            .ok()
            .and_then(|index| row.question.players.get(index))
            .map(|name| name.trim().to_lowercase())
        else {
            continue;
        };
        seq.push(i32::try_from(seq.len()).map_err(anyhow::Error::from)?);
        game_no.push(row.question.reference.game_no);
        decision_no.push(row.question.reference.decision_no);
        player_name.push(name);
        question_kind.push(row.question.kind.clone());
        game_plan.push(crate::position_game_plans::label(row.plan));
        opportunities.push(opportunity(&row.decision).map(super::history::kind_name));
        mistake_kind.push(super::history::kind_name(kind(&row.decision)));
        equity_loss.push(row.decision.equity_loss);
        match_length.push(context.match_length);
        away_own.push(context.away.map(|away| away[0]));
        away_opponent.push(context.away.map(|away| away[1]));
        cube_value.push(context.cube_value);
        cube_owner.push(owner_filter(&context, row.decision.player).name());
        let practicable = row.decision.equity_loss > 0.0 || can_practice_correct_cube(&row);
        hits.push(
            serde_json::to_value(DecisionHit::new(row, &context, practicable))
                .map_err(anyhow::Error::from)?,
        );
    }
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    // Concurrent requests for the same match must not interleave their rows.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 72))")
        .bind(candidate.id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    // The rows describe the decisions saved under the candidate's fingerprint.
    // A slower request must not replace a newer entry with them.
    let current: Option<Candidate> = sqlx::query_as(&format!("{CANDIDATES} AND m.id=$2"))
        .bind(version())
        .bind(candidate.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    if let Some(current) = current
        && current.fingerprint != candidate.fingerprint
    {
        if current.stored.as_deref() == Some(current.fingerprint.as_str()) {
            return Ok(Published::Current);
        }
        return Ok(Published::Outdated(current));
    }
    sqlx::query("DELETE FROM decision_index_matches WHERE match_id=$1")
        .bind(candidate.id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    // The match may have been deleted since the fingerprint was read.
    let inserted = sqlx::query(
        "INSERT INTO decision_index_matches(match_id,fingerprint)
         SELECT id,$2 FROM matches WHERE id=$1",
    )
    .bind(candidate.id)
    .bind(&candidate.fingerprint)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?
    .rows_affected();
    if inserted == 1 && !seq.is_empty() {
        sqlx::query(
            "INSERT INTO decision_index(match_id,seq,game_no,decision_no,player_name,question_kind,
                game_plan,opportunity,mistake_kind,equity_loss,match_length,away_own,away_opponent,
                cube_value,cube_owner,played_on,hit)
             SELECT $1,u.seq,u.game_no,u.decision_no,u.player_name,u.question_kind,u.game_plan,
                    u.opportunity,u.mistake_kind,u.equity_loss,u.match_length,u.away_own,
                    u.away_opponent,u.cube_value,u.cube_owner,$16,u.hit
             FROM unnest($2::int[],$3::int[],$4::int[],$5::text[],$6::text[],$7::text[],
                         $8::text[],$9::text[],$10::float8[],$11::int[],$12::int[],$13::int[],
                         $14::int[],$15::text[],$17::jsonb[])
                  AS u(seq,game_no,decision_no,player_name,question_kind,game_plan,opportunity,
                       mistake_kind,equity_loss,match_length,away_own,away_opponent,cube_value,
                       cube_owner,hit)",
        )
        .bind(candidate.id)
        .bind(&seq)
        .bind(&game_no)
        .bind(&decision_no)
        .bind(&player_name)
        .bind(&question_kind)
        .bind(&game_plan)
        .bind(&opportunities)
        .bind(&mistake_kind)
        .bind(&equity_loss)
        .bind(&match_length)
        .bind(&away_own)
        .bind(&away_opponent)
        .bind(&cube_value)
        .bind(&cube_owner)
        .bind(candidate.played)
        .bind(&hits)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Published::Current)
}

/// Decisions needed on each side of the comparison before a trend is shown.
const MIN_TREND_DECISIONS: i64 = 10;

#[derive(FromRow)]
struct GroupRow {
    game_plan: String,
    kind: String,
    errors: i64,
    matches: i64,
    loss: f64,
    earlier_errors: i64,
    recent_errors: i64,
    decisions: Option<i64>,
    earlier_decisions: Option<i64>,
    recent_decisions: Option<i64>,
}

/// A match played without learning aids: an import, or a bot match in
/// Competition mode that used no hint, Coach, Tutor or Training feedback.
/// The match database applies the same rule before it shows a PR.
pub(super) const INDEPENDENT_MATCH: &str =
    "(NOT m.play_assisted AND coalesce(m.play_mode,'competition')='competition')";

/// Aggregate one player's indexed decisions into mistake groups.
/// Returns the groups, the evaluable decisions and the matches they cover.
pub(super) async fn groups(
    pool: &PgPool,
    database: Option<Uuid>,
    player: &[String],
    plan: Option<crate::game_plan::GamePlan>,
    min_loss: f64,
    independent_only: bool,
) -> ApiResult<(Vec<super::MistakeGroup>, usize, usize)> {
    // The player's matches are split by date into an earlier and a recent
    // half, so each group can show whether its error rate is moving.
    let scoped = format!(
        "WITH scoped AS (
            SELECT d.match_id,d.game_plan,d.opportunity,d.mistake_kind,d.equity_loss,d.played_on
            FROM decision_index d JOIN matches m ON m.id=d.match_id
            WHERE d.player_name=ANY($1) AND ($2::uuid IS NULL OR m.database_id=$2)
              AND ($3::text IS NULL OR d.game_plan=$3)
              AND (NOT $4 OR {INDEPENDENT_MATCH})
        ), halves AS (
            SELECT match_id,ntile(2) OVER (ORDER BY min(played_on),match_id) AS half
            FROM scoped GROUP BY match_id
        )"
    );
    let plan = plan.map(crate::position_game_plans::label);
    let (decisions, matches): (i64, i64) = sqlx::query_as(&format!(
        "{scoped} SELECT count(*),count(DISTINCT match_id) FROM scoped"
    ))
    .bind(player)
    .bind(database)
    .bind(plan)
    .bind(independent_only)
    .fetch_one(pool)
    .await
    .map_err(anyhow::Error::from)?;
    let rows: Vec<GroupRow> = sqlx::query_as(&format!(
        "{scoped}, opportunities AS (
            SELECT s.game_plan,s.opportunity AS kind,count(*) AS decisions,
                   count(*) FILTER (WHERE h.half=1) AS earlier_decisions,
                   count(*) FILTER (WHERE h.half=2) AS recent_decisions
            FROM scoped s JOIN halves h USING (match_id)
            WHERE s.opportunity IS NOT NULL GROUP BY 1,2
        ), errors AS (
            SELECT s.game_plan,s.mistake_kind AS kind,count(*) AS errors,
                   count(DISTINCT s.match_id) AS matches,sum(s.equity_loss) AS loss,
                   count(*) FILTER (WHERE h.half=1) AS earlier_errors,
                   count(*) FILTER (WHERE h.half=2) AS recent_errors
            FROM scoped s JOIN halves h USING (match_id)
            WHERE s.equity_loss>=$5 AND s.equity_loss>0 GROUP BY 1,2
        )
        SELECT e.game_plan,e.kind,e.errors,e.matches,e.loss,e.earlier_errors,e.recent_errors,
               o.decisions,o.earlier_decisions,o.recent_decisions
        FROM errors e LEFT JOIN opportunities o USING (game_plan,kind)
        ORDER BY e.loss DESC,e.game_plan,e.kind"
    ))
    .bind(player)
    .bind(database)
    .bind(plan)
    .bind(independent_only)
    .bind(min_loss)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    let count = |value: i64| usize::try_from(value).unwrap_or(0);
    let mut groups = Vec::with_capacity(rows.len());
    for row in rows {
        let game_plan = serde_json::from_value(serde_json::Value::String(row.game_plan))
            .map_err(anyhow::Error::from)?;
        let mistake_kind = serde_json::from_value(serde_json::Value::String(row.kind))
            .map_err(anyhow::Error::from)?;
        let rates = row.decisions.filter(|n| *n > 0).map(|decisions| {
            let trend = row
                .earlier_decisions
                .zip(row.recent_decisions)
                .filter(|(earlier, recent)| {
                    *earlier >= MIN_TREND_DECISIONS && *recent >= MIN_TREND_DECISIONS
                })
                .map(|(earlier, recent)| super::MistakeTrend {
                    earlier_error_rate: row.earlier_errors as f64 / earlier as f64,
                    recent_error_rate: row.recent_errors as f64 / recent as f64,
                    earlier_decisions: count(earlier),
                    recent_decisions: count(recent),
                });
            super::MistakeRates {
                decision_count: count(decisions),
                error_rate: row.errors as f64 / decisions as f64,
                loss_per_100: row.loss / decisions as f64 * 100.0,
                small_sample: count(decisions) < super::MIN_RANKED_DECISIONS,
                trend,
            }
        });
        groups.push(super::MistakeGroup {
            game_plan,
            mistake_kind,
            count: count(row.errors),
            match_count: count(row.matches),
            total_loss: row.loss,
            rates,
        });
    }
    Ok((groups, count(decisions), count(matches)))
}

/// The player's matches in the scope and how many of them have no indexed
/// decision. Call after `ensure`, which brings the index up to date.
pub(super) async fn coverage(
    pool: &PgPool,
    database: Option<Uuid>,
    player: &[String],
    independent_only: bool,
) -> ApiResult<(usize, usize)> {
    let (matches, unanalyzed): (i64, i64) = sqlx::query_as(&format!(
        "SELECT count(*),
                count(*) FILTER (WHERE NOT EXISTS(
                    SELECT 1 FROM decision_index d WHERE d.match_id=m.id))
         FROM matches m
         WHERE NOT m.collection_only AND ($1::uuid IS NULL OR m.database_id=$1)
           AND (lower(btrim(m.white))=ANY($2) OR lower(btrim(m.black))=ANY($2))
           AND (NOT $3 OR {INDEPENDENT_MATCH})"
    ))
    .bind(database)
    .bind(player)
    .bind(independent_only)
    .fetch_one(pool)
    .await
    .map_err(anyhow::Error::from)?;
    let count = |value: i64| usize::try_from(value).unwrap_or(0);
    Ok((count(matches), count(unanalyzed)))
}

/// Of the given matches, those played without learning aids.
pub(super) async fn independent_matches(
    pool: &PgPool,
    matches: &[Uuid],
) -> ApiResult<std::collections::HashSet<Uuid>> {
    let ids: Vec<Uuid> = sqlx::query_scalar(&format!(
        "SELECT m.id FROM matches m WHERE m.id=ANY($1) AND {INDEPENDENT_MATCH}"
    ))
    .bind(matches)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(ids.into_iter().collect())
}

pub(super) async fn ensure_and_group(
    pool: &PgPool,
    database: Option<Uuid>,
    player: &[String],
    plan: Option<crate::game_plan::GamePlan>,
    min_loss: f64,
    independent_only: bool,
) -> ApiResult<(Vec<super::MistakeGroup>, usize, usize)> {
    ensure(pool, database, player).await?;
    groups(pool, database, player, plan, min_loss, independent_only).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str =
        "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])";

    fn ok<T>(result: ApiResult<T>) -> T {
        result.unwrap_or_else(|ApiError(status, message)| panic!("{status}: {message}"))
    }

    async fn current(pool: &PgPool, id: Uuid) -> Candidate {
        sqlx::query_as(&format!("{CANDIDATES} AND m.id=$2"))
            .bind(version())
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn entry(pool: &PgPool, id: Uuid) -> Option<(String, String)> {
        sqlx::query_as(
            "SELECT fingerprint,indexed_at::text FROM decision_index_matches WHERE match_id=$1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn slower_rebuilds_never_publish_an_older_fingerprint(pool: PgPool) {
        let parsed = crate::import::parse_sgf(SOURCE).unwrap();
        let id = crate::store::import_match(
            &pool,
            "race.sgf",
            "sgf",
            SOURCE.as_bytes(),
            SOURCE,
            &parsed,
        )
        .await
        .unwrap()
        .r#match
        .id;
        // `ensure` catalogs older imports before it reads fingerprints.
        crate::move_catalog::ensure_catalog(&pool, id)
            .await
            .unwrap();
        crate::imported_analysis::ensure_catalog(&pool, id)
            .await
            .unwrap();
        let fresh = current(&pool, id).await;
        // A request that read the fingerprint before the decisions changed.
        let read_before_change = Candidate {
            fingerprint: format!("{}-before", fresh.fingerprint),
            ..fresh.clone()
        };

        let Published::Outdated(next) = ok(store(&pool, &read_before_change, Vec::new()).await)
        else {
            panic!("an outdated fingerprint was published");
        };
        assert_eq!(next.fingerprint, fresh.fingerprint);
        assert_eq!(entry(&pool, id).await, None);

        // The rebuild loads again under the current fingerprint.
        ok(publish(&pool, read_before_change.clone()).await);
        let published = entry(&pool, id).await.unwrap();
        assert_eq!(published.0, fresh.fingerprint);

        // Once the current state is stored, a slower request leaves it alone.
        assert!(matches!(
            ok(store(&pool, &read_before_change, Vec::new()).await),
            Published::Current
        ));
        assert_eq!(entry(&pool, id).await, Some(published));
    }
}
