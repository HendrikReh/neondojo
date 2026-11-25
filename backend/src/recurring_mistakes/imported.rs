//! Bounded foreign catalogs; native checkpoint ownership is decided before projection.
use super::{
    MistakePracticeQuestion, MistakeReference, MistakeSource, imported_projection,
    source::{
        CHOSEN, ELIGIBLE, MAX_DECISIONS, MAX_GAMES, MAX_SOURCE_BYTES, SourceDecision, TOO_BROAD,
    },
};
use crate::{
    api::{ApiError, ApiResult},
    game_plan,
    import::{MatchFormat, supplies_decisions_sql},
    imported_analysis::ImportedAnalysisData,
};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

const MAX_LEGACY_SOURCES: usize = 16;
const DECISION_FORMATS: &str = supplies_decisions_sql!("m");
const FOREIGN: &str = concat!(
    "FROM eligible m JOIN match_games g ON g.match_uuid=m.id WHERE ",
    supplies_decisions_sql!("m"),
    " AND EXISTS(SELECT 1 FROM effective_analysis_selections selected WHERE selected.match_id=m.id AND selected.source='imported')"
);
/// Older foreign imports whose catalogs are still missing. The pending filter
/// matches the partial index `matches_pending_imported_catalog`, so a library
/// without such imports answers from an empty index.
const LEGACY_PENDING: &str = concat!(
    "FROM eligible m LEFT JOIN match_games g ON g.match_uuid=m.id WHERE ",
    supplies_decisions_sql!("m"),
    " AND (NOT m.imported_analysis_loaded OR NOT m.moves_cataloged)"
);

/// Pending games and the largest pending source; `$1` database, `$2` match, `$3` players.
pub(super) fn legacy_size_sql() -> String {
    format!(
        "{ELIGIBLE} SELECT count(*),coalesce(max(octet_length(m.original)::bigint+octet_length(m.sgf)::bigint),0)::bigint {LEGACY_PENDING}"
    )
}

/// The first pending matches, one more than a request catalogs.
pub(super) fn legacy_ids_sql() -> String {
    format!(
        "{ELIGIBLE} SELECT DISTINCT m.id {LEGACY_PENDING} ORDER BY m.id LIMIT {}",
        MAX_LEGACY_SOURCES + 1
    )
}

/// Player choices use metadata only. Catalog legacy sources only once their
/// participant/match is selected, and reject oversized requests before parsing.
pub(super) async fn catalog_legacy(
    pool: &PgPool,
    database: Option<Uuid>,
    match_id: Option<Uuid>,
    player: Option<&[String]>,
) -> ApiResult<()> {
    let (games, bytes): (i64, i64) = sqlx::query_as(&legacy_size_sql())
        .bind(database)
        .bind(match_id)
        .bind(player.map(<[String]>::to_vec))
        .fetch_one(pool)
        .await
        .map_err(anyhow::Error::from)?;
    if games > MAX_GAMES as i64 || bytes > MAX_SOURCE_BYTES {
        return Err(ApiError::invalid(TOO_BROAD));
    }
    let ids: Vec<Uuid> = sqlx::query_scalar(&legacy_ids_sql())
        .bind(database)
        .bind(match_id)
        .bind(player.map(<[String]>::to_vec))
        .fetch_all(pool)
        .await
        .map_err(anyhow::Error::from)?;
    let more = ids.len() > MAX_LEGACY_SOURCES;
    for id in ids.into_iter().take(MAX_LEGACY_SOURCES) {
        // Failure is propagated: incomplete legacy coverage must not look complete.
        // Imported decision rows reference the game catalog, which can also be absent.
        crate::move_catalog::ensure_catalog(pool, id).await?;
        crate::imported_analysis::ensure_catalog(pool, id).await?;
    }
    if more {
        return Err(ApiError::invalid(
            "Cataloged 16 legacy sources; retry this request to finish cataloging all selected sources",
        ));
    }
    Ok(())
}
#[derive(FromRow)]
pub(super) struct ImportedGame {
    cube_enabled: Option<bool>,
    match_id: Uuid,
    game_no: i32,
    format: MatchFormat,
    white: String,
    black: String,
}
#[derive(FromRow)]
pub(super) struct ImportedRow {
    id: Uuid,
    match_id: Uuid,
    position_uuid: Option<Uuid>,
    game_no: i32,
    source_move_no: i32,
    kind: String,
    data: serde_json::Value,
}
#[derive(FromRow)]
struct ImportedEvent {
    #[sqlx(flatten)]
    event: crate::move_catalog::StoredMove,
    checker_ply: Option<i32>,
}
pub(super) struct Imported {
    games: Vec<ImportedGame>,
    rows: Vec<ImportedRow>,
    events: Vec<ImportedEvent>,
}
pub(super) async fn load(
    tx: &mut Transaction<'_, Postgres>,
    database: Option<Uuid>,
    match_id: Option<Uuid>,
    player: Option<&[String]>,
    native_games: usize,
    native_decisions: usize,
) -> ApiResult<Imported> {
    let count: i64 = sqlx::query_scalar(&format!("{CHOSEN} SELECT count(*) {FOREIGN}"))
        .bind(database)
        .bind(match_id)
        .bind(player.map(<[String]>::to_vec))
        .fetch_one(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?;
    if count + native_games as i64 > MAX_GAMES as i64 {
        return Err(ApiError::invalid(TOO_BROAD));
    }
    let games=sqlx::query_as(&format!("{CHOSEN} SELECT m.id AS match_id,g.game_no,g.cube_enabled,m.format,m.white,m.black {FOREIGN} ORDER BY m.id,g.game_no"))
        .bind(database).bind(match_id).bind(player.map(<[String]>::to_vec)).fetch_all(&mut **tx).await.map_err(anyhow::Error::from)?;
    let events:Vec<ImportedEvent>=sqlx::query_as(&format!("{CHOSEN} SELECT e.*,p.ply AS checker_ply FROM eligible m JOIN match_moves e ON e.match_uuid=m.id LEFT JOIN positions p ON p.id=e.position_uuid AND p.match_uuid=e.match_uuid AND p.game_no=e.game_no WHERE {DECISION_FORMATS} AND EXISTS(SELECT 1 FROM effective_analysis_selections selected WHERE selected.match_id=m.id AND selected.source='imported') ORDER BY m.id,e.game_no,e.move_no LIMIT {}",MAX_DECISIONS+1))
        .bind(database).bind(match_id).bind(player.map(<[String]>::to_vec)).fetch_all(&mut **tx).await.map_err(anyhow::Error::from)?;
    if events.len() > MAX_DECISIONS {
        return Err(ApiError::invalid(TOO_BROAD));
    }
    let rows:Vec<ImportedRow>=sqlx::query_as(&format!("{CHOSEN} SELECT a.id,a.match_uuid AS match_id,a.position_uuid,a.game_no,a.source_move_no,a.kind,a.data FROM eligible m JOIN primary_imported_analyses a ON a.match_uuid=m.id WHERE {DECISION_FORMATS} AND a.kind IN ('checker','cube') AND EXISTS(SELECT 1 FROM effective_analysis_selections selected WHERE selected.match_id=m.id AND selected.source='imported') ORDER BY m.id,a.game_no,a.source_move_no,a.source_ordinal LIMIT {}",MAX_DECISIONS+1))
        .bind(database).bind(match_id).bind(player.map(<[String]>::to_vec)).fetch_all(&mut **tx).await.map_err(anyhow::Error::from)?;
    if rows.len() + native_decisions > MAX_DECISIONS {
        return Err(ApiError::invalid(TOO_BROAD));
    }
    Ok(Imported {
        games,
        rows,
        events,
    })
}
pub(super) fn reconstruct(imported: Imported) -> Vec<SourceDecision> {
    // match_moves is the immutable catalog produced by parse_sgf_source_moves.
    // Reuse it so analysis-bearing SGFs are not parsed or loaded on every request.
    let mut moves = HashMap::new();
    for ImportedEvent { event, checker_ply } in imported.events {
        if let Some(source) = imported_projection::source_move(&event, checker_ply) {
            moves.insert(
                (event.match_uuid, event.game_no, event.move_no),
                (event.position_uuid, source),
            );
        }
    }
    let mut by_game = HashMap::<_, Vec<_>>::new();
    for row in imported.rows {
        by_game
            .entry((row.match_id, row.game_no))
            .or_default()
            .push(row);
    }
    let mut output = vec![];
    for game in imported.games {
        let mut seen = HashSet::new();
        for row in by_game
            .remove(&(game.match_id, game.game_no))
            .unwrap_or_default()
        {
            let Ok(record) = serde_json::from_value::<ImportedAnalysisData>(row.data) else {
                continue;
            };
            if record.game_no != row.game_no
                || record.source_move_no != row.source_move_no
                || record.kind != row.kind
                || !seen.insert((row.source_move_no, row.kind.clone()))
            {
                continue;
            }
            let Some((position_uuid, source)) =
                moves.get(&(row.match_id, row.game_no, row.source_move_no))
            else {
                continue;
            };
            if row.position_uuid != *position_uuid {
                continue;
            }
            let Some(p) = imported_projection::project(
                &record,
                source,
                game.match_id,
                row.position_uuid,
                game.format,
                game.cube_enabled,
            ) else {
                continue;
            };
            let Ok(board) = <&[i32; 26]>::try_from(p.position.board.as_slice()) else {
                continue;
            };
            let Ok(plans) = game_plan::classify(board) else {
                continue;
            };
            let plan = if p.decision.player == 0 {
                plans.white
            } else {
                plans.black
            };
            let kind = if p.decision.kind == "checker" {
                "checker"
            } else if matches!(p.decision.played.as_str(), "take" | "drop") {
                "cube_response"
            } else {
                "cube_offer"
            };
            let method = if record.kind == "checker" {
                record
                    .candidates
                    .iter()
                    .find(|c| c.rank == 1)
                    .and_then(|c| c.method.as_deref())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(&record.method)
                    .to_owned()
            } else {
                record.method.clone()
            };
            let engine = match record
                .engine_version
                .as_deref()
                .filter(|s| !s.trim().is_empty())
            {
                Some(version) => format!("{} {}", record.engine, version),
                None => record.engine,
            };
            output.push(SourceDecision {
                plan,
                original_match_state: None,
                decision: p.decision,
                question: MistakePracticeQuestion {
                    reference: MistakeReference {
                        job_id: None,
                        imported_analysis_id: Some(row.id),
                        game_no: row.game_no,
                        decision_no: row.source_move_no,
                    },
                    kind: kind.into(),
                    position: p.position,
                    players: [game.white.clone(), game.black.clone()],
                    engine,
                    plies: p.depth_plies,
                    depth: p.depth,
                    method: Some(method),
                    source: Some(match game.format {
                        MatchFormat::Xg => MistakeSource::Xg,
                        MatchFormat::Bgf => MistakeSource::Bgblitz,
                        MatchFormat::Sgf | MatchFormat::Mat | MatchFormat::Ogxm => {
                            MistakeSource::Sgf
                        }
                    }),
                },
            });
        }
    }
    output
}
