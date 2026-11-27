//! Bounded source selection and exact SGF decision reconstruction.
use super::{MistakePracticeQuestion, MistakeReference, MistakeSource};
use crate::{
    api::{ApiError, ApiResult},
    backgammon::ids,
    game_plan::{self, GamePlan},
    match_analysis::Decision,
    model::Position,
};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;
pub(super) const MAX_GAMES: usize = 5_000;
pub(super) const MAX_DECISIONS: usize = 200_000;
pub(super) const MAX_SOURCE_BYTES: i64 = 64 * 1024 * 1024;
pub(crate) const TOO_BROAD: &str =
    "Too many analyzed decisions or source games; choose a database to narrow this request";

pub(crate) fn evaluated(d: &Decision) -> bool {
    crate::match_analysis::evaluated_equities(d).is_some()
}
pub(crate) struct SourceDecision {
    pub(crate) question: MistakePracticeQuestion,
    pub(crate) decision: Decision,
    pub(crate) plan: GamePlan,
    pub(crate) original_match_state: Option<String>,
}
impl SourceDecision {
    /// `players` holds the lowercase names of one player's profile.
    pub(crate) fn in_scope(&self, players: &[String], plan: Option<GamePlan>) -> bool {
        players.contains(
            &self.question.players[self.decision.player as usize]
                .trim()
                .to_lowercase(),
        ) && plan.is_none_or(|p| p == self.plan)
    }

    pub(crate) fn meets_loss_threshold(&self, loss: f64) -> bool {
        self.decision.equity_loss >= loss && self.decision.equity_loss > 0.0
    }
}
#[derive(FromRow)]
struct Game {
    job_id: Uuid,
    match_id: Uuid,
    game_no: i32,
    plies: i32,
    engine: String,
    sgf: String,
    white: String,
    black: String,
}
#[derive(FromRow)]
struct Row {
    job_id: Uuid,
    game_no: i32,
    decision_no: i32,
    position_uuid: Option<Uuid>,
    player: i32,
    kind: String,
    equity_loss: f64,
    data: serde_json::Value,
}
/// The library matches a request covers: `$1` database, `$2` match, `$3`
/// lowercase player names; each may be NULL. Matches in a live competition
/// session are left out until it ends.
macro_rules! eligible {
    () => {
        concat!(
            r#"
WITH eligible AS (
    SELECT m.* FROM matches m
    WHERE NOT m.collection_only
      AND ($1::uuid IS NULL OR m.database_id=$1)
      AND ($2::uuid IS NULL OR m.id=$2)
      AND ($3::text[] IS NULL OR lower(btrim(m.white))=ANY($3) OR lower(btrim(m.black))=ANY($3))
      AND NOT "#,
            crate::play::review_blocked_sql!("m.id"),
            "\n)"
        )
    };
}
/// Only `eligible`. A query that reads no other CTE of `CHOSEN` uses this, so
/// PostgreSQL can inline it and apply the query's own filters to `matches`.
pub(super) const ELIGIBLE: &str = eligible!();
// Pick a job for the entire game, including when its best saved decisions are
// unavailable. A lower-depth job must never fill holes or replace smaller losses.
pub(super) const CHOSEN: &str = concat!(
    eligible!(),
    r#", chosen AS (
    SELECT
        j.id AS job_id,m.id AS match_id,c.game_no,j.plies,
        catalog.engine
    FROM eligible m
    -- The catalog views aggregate every analysis of a match. OFFSET 0 keeps
    -- each lookup scoped to one eligible match; a plain join makes the planner
    -- build the catalog for the whole library first.
    CROSS JOIN LATERAL (
        SELECT s.analysis_id,s.job_id FROM effective_analysis_selections s
        WHERE s.match_id=m.id OFFSET 0
    ) selected
    CROSS JOIN LATERAL (
        SELECT a.engine FROM analysis_catalog_sources a
        WHERE a.match_id=m.id AND a.analysis_id=selected.analysis_id OFFSET 0
    ) catalog
    JOIN jobs j ON j.id=selected.job_id AND j.match_uuid=m.id
    JOIN match_analysis_games c ON c.job_id=j.id
    WHERE j.kind='match' AND j.status='completed'
      AND j.source_revision=m.source_revision
      AND (j.game_no IS NULL OR j.game_no=c.game_no)
    ORDER BY m.id,c.game_no
)
"#
);

pub(crate) async fn load(
    pool: &PgPool,
    database: Option<Uuid>,
    match_id: Option<Uuid>,
    player: Option<&[String]>,
) -> ApiResult<Vec<SourceDecision>> {
    super::imported::catalog_legacy(pool, database, match_id, player).await?;
    let mut tx = crate::database_pool::begin_read_snapshot(pool).await?;
    let saved = read_saved_decisions(&mut tx, database, match_id, player).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    saved.project().await
}

pub(super) struct SavedDecisions {
    games: Vec<Game>,
    rows: Vec<Row>,
    imported: super::imported::Imported,
}

pub(super) async fn read_saved_decisions(
    tx: &mut Transaction<'_, Postgres>,
    database: Option<Uuid>,
    match_id: Option<Uuid>,
    player: Option<&[String]>,
) -> ApiResult<SavedDecisions> {
    if let Some(id) = database {
        require_database(&mut **tx, id).await?;
    }
    let (games, rows) = native_rows(tx, database, match_id, player).await?;
    let imported =
        super::imported::load(tx, database, match_id, player, games.len(), rows.len()).await?;
    Ok(SavedDecisions {
        games,
        rows,
        imported,
    })
}

impl SavedDecisions {
    pub(super) async fn project(self) -> ApiResult<Vec<SourceDecision>> {
        tokio::task::spawn_blocking(move || {
            let mut output = reconstruct(self.games, self.rows)?;
            output.extend(super::imported::reconstruct(self.imported));
            Ok(output)
        })
        .await
        .map_err(anyhow::Error::from)?
    }
}
/// Read the same completed native checkpoints without cataloging imported
/// analysis. The checkpoint count also distinguishes an empty analysis from a
/// match that has never been analyzed.
pub(crate) async fn load_native_match(
    pool: &PgPool,
    match_id: Uuid,
) -> ApiResult<(bool, Vec<SourceDecision>)> {
    let mut tx = crate::database_pool::begin_read_snapshot(pool).await?;
    let (games, rows) = native_rows(&mut tx, None, Some(match_id), None).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    let has_analysis = !games.is_empty();
    let decisions = tokio::task::spawn_blocking(move || reconstruct(games, rows))
        .await
        .map_err(anyhow::Error::from)??;
    Ok((has_analysis, decisions))
}

pub(super) async fn require_database<'e>(db: impl sqlx::PgExecutor<'e>, id: Uuid) -> ApiResult<()> {
    crate::databases::require_library_database_invalid(db, id, "Choose an existing database").await
}

async fn native_rows(
    tx: &mut Transaction<'_, Postgres>,
    database: Option<Uuid>,
    match_id: Option<Uuid>,
    player: Option<&[String]>,
) -> ApiResult<(Vec<Game>, Vec<Row>)> {
    // A checkpoint belongs to a game, but the artifact belongs to the match.
    // Account for and transfer each match's source only once, even when hundreds
    // of its games have selected checkpoints.
    let (games,bytes):(i64,i64)=sqlx::query_as(&format!("{CHOSEN} SELECT count(*),(SELECT coalesce(sum(octet_length(m.sgf)),0)::bigint FROM matches m WHERE m.id IN (SELECT match_id FROM chosen)) FROM chosen")).bind(database).bind(match_id).bind(player.map(<[String]>::to_vec)).fetch_one(&mut **tx).await.map_err(anyhow::Error::from)?;
    if games > MAX_GAMES as i64 || bytes > MAX_SOURCE_BYTES {
        return Err(ApiError::invalid(TOO_BROAD));
    }
    let games:Vec<Game>=sqlx::query_as(&format!("{CHOSEN} SELECT c.*,CASE WHEN row_number() OVER (PARTITION BY c.match_id ORDER BY c.game_no)=1 THEN m.sgf ELSE '' END AS sgf,m.white,m.black FROM chosen c JOIN matches m ON m.id=c.match_id ORDER BY c.match_id,c.game_no LIMIT {}",MAX_GAMES+1)).bind(database).bind(match_id).bind(player.map(<[String]>::to_vec)).fetch_all(&mut **tx).await.map_err(anyhow::Error::from)?;
    let rows:Vec<Row>=sqlx::query_as(&format!("{CHOSEN} SELECT a.job_id,a.game_no,a.decision_no,a.position_uuid,a.player,a.kind,a.equity_loss,a.data FROM chosen c JOIN move_analyses a ON a.job_id=c.job_id AND a.game_no=c.game_no ORDER BY c.match_id,c.game_no,a.decision_no LIMIT {}",MAX_DECISIONS+1)).bind(database).bind(match_id).bind(player.map(<[String]>::to_vec)).fetch_all(&mut **tx).await.map_err(anyhow::Error::from)?;
    if rows.len() > MAX_DECISIONS {
        return Err(ApiError::invalid(TOO_BROAD));
    }
    Ok((games, rows))
}

pub(super) async fn players(pool: &PgPool, database: Option<Uuid>) -> ApiResult<Vec<String>> {
    if let Some(id) = database {
        require_database(pool, id).await?;
    }
    let names: Vec<String> = sqlx::query_scalar(&format!(
        "{ELIGIBLE}
         SELECT DISTINCT name FROM eligible m
         CROSS JOIN LATERAL (VALUES(btrim(m.white)), (btrim(m.black))) names(name)
         WHERE name<>'' ORDER BY name LIMIT 10001"
    ))
    .bind(database)
    .bind(None::<Uuid>)
    .bind(None::<Vec<String>>)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    if names.len() > 10000 {
        return Err(ApiError::invalid(
            "Too many available players; choose a database to narrow this request",
        ));
    }
    let names = crate::player_scope::choices(pool, names).await?;
    let mut unique = BTreeMap::new();
    for name in names {
        unique.entry(name.to_lowercase()).or_insert(name);
    }
    Ok(unique.into_values().collect())
}
fn reconstruct(games: Vec<Game>, rows: Vec<Row>) -> ApiResult<Vec<SourceDecision>> {
    let mut by_game = HashMap::<(Uuid, i32), Vec<Row>>::new();
    for row in rows {
        by_game
            .entry((row.job_id, row.game_no))
            .or_default()
            .push(row);
    }
    let mut contexts = HashMap::new();
    let mut output = vec![];
    for game in games {
        if let std::collections::hash_map::Entry::Vacant(entry) = contexts.entry(game.match_id) {
            entry.insert(
                crate::import::sgf::parse_sgf_with_context(&game.sgf)
                    .ok()
                    .map(|(_, c)| c),
            );
        }
        let Some(context) = contexts
            .get(&game.match_id)
            .and_then(Option::as_ref)
            .and_then(|c| {
                usize::try_from(game.game_no - 1)
                    .ok()
                    .and_then(|n| c.get(n))
            })
        else {
            continue;
        };
        let mut action = 0;
        let mut game_output = vec![];
        let mut valid = true;
        for (index, row) in by_game
            .remove(&(game.job_id, game.game_no))
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            // A missing or malformed row cannot shift later questions to a wrong board.
            if row.decision_no != index as i32 + 1 {
                valid = false;
                break;
            }
            let Some((position_id, match_state_id)) = context.decision_ids.get(action) else {
                valid = false;
                break;
            };
            if row.data.get("played").and_then(|p| p.as_str()) != Some("no double") {
                action += 1;
            }
            let Ok(d) = serde_json::from_value::<Decision>(row.data) else {
                continue;
            };
            if (!context.cube_enabled && d.kind == "cube")
                || d.decision_no != row.decision_no
                || d.player != row.player
                || d.kind != row.kind
                || !row.equity_loss.is_finite()
                || (d.equity_loss - row.equity_loss).abs() > 0.000000001
                || !evaluated(&d)
            {
                continue;
            }
            if ids::on_roll(match_state_id) != Some(d.player) {
                valid = false;
                break;
            }
            let Ok(board) = ids::decode_board(position_id, match_state_id) else {
                continue;
            };
            let Some(plan) = game_plan::classify_for_player(&board, d.player) else {
                continue;
            };
            let question_kind = crate::cube_context::decision_question_kind(&d.kind, &d.played);
            let dice = if d.kind == "checker" {
                let Some(dice) = d
                    .dice
                    .filter(|dice| dice.iter().all(|n| (1..=6).contains(n)))
                else {
                    continue;
                };
                dice.to_vec()
            } else {
                vec![0, 0]
            };
            let question_match_id = if d.kind == "cube" {
                ids::hide_dice(match_state_id)
            } else {
                Some(match_state_id.clone())
            }
            .and_then(|id| {
                if question_kind == "cube_response" {
                    ids::with_offer_marker(&id)
                } else {
                    Some(id)
                }
            });
            let Some(question_match_id) = question_match_id else {
                valid = false;
                break;
            };
            game_output.push(SourceDecision {
                plan,
                original_match_state: Some(match_state_id.clone()),
                question: MistakePracticeQuestion {
                    reference: MistakeReference {
                        job_id: Some(game.job_id),
                        imported_analysis_id: None,
                        game_no: game.game_no,
                        decision_no: d.decision_no,
                    },
                    kind: question_kind.into(),
                    position: Position {
                        cube_enabled: Some(context.cube_enabled),
                        id: row.position_uuid.unwrap_or(Uuid::nil()),
                        match_uuid: game.match_id,
                        ordinal: d.checker_no.unwrap_or(d.decision_no),
                        game_no: game.game_no,
                        ply: d.checker_no.unwrap_or(0),
                        player: d.player,
                        dice,
                        board,
                        position_id: position_id.clone(),
                        gnubg_match_id: question_match_id,
                        notation: String::new(),
                    },
                    players: [game.white.clone(), game.black.clone()],
                    engine: game.engine.clone(),
                    plies: Some(game.plies),
                    depth: Some(format!("{}-ply", game.plies)),
                    method: None,
                    source: Some(MistakeSource::Native),
                },
                decision: d,
            });
        }
        if valid {
            output.extend(game_output);
        }
    }
    Ok(output)
}
