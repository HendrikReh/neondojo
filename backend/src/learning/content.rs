mod decision_batch;
mod match_cards;
pub(super) use decision_batch::decision_cards;
use match_cards::MatchCardSource;

use super::*;
use crate::recurring_mistakes::MistakeReference;
use crate::{
    analysis_catalog::{self, SelectedAnalysis},
    backgammon::ids::{self, decode_board},
    engine::Analysis,
    import::MatchFormat,
    move_assessments::after_play,
    store,
};
use anyhow::{Context, ensure};
use std::collections::BTreeSet;

pub(super) async fn from_source(pool: &PgPool, request: &AddCards) -> ApiResult<Vec<CardContent>> {
    if let Some(id) = request.position_id {
        return from_position(pool, id, request).await;
    }
    let match_id = if let Some(id) = request.match_id {
        id
    } else if let Some(id) = request.job_id {
        store::get_job(pool, id)
            .await?
            .and_then(|job| job.match_uuid)
            .ok_or_else(|| ApiError::invalid("job_id must refer to a match analysis"))?
    } else {
        return Err(ApiError::invalid(
            "Supply a position_id, match_id, or job_id",
        ));
    };
    let legacy_id = request.job_id.map(|id| format!("job:{id}"));
    if request
        .analysis_id
        .as_ref()
        .zip(legacy_id.as_ref())
        .is_some_and(|(a, b)| a != b)
    {
        return Err(ApiError::invalid(
            "job_id and analysis_id must identify the same analysis",
        ));
    }
    match analysis_catalog::resolve_selection(
        pool,
        match_id,
        request.analysis_id.as_deref().or(legacy_id.as_deref()),
    )
    .await?
    {
        Some(SelectedAnalysis::Job(job)) => from_match(pool, job.id, request).await,
        Some(SelectedAnalysis::Imported { .. }) => imported_match(pool, match_id, request).await,
        None => Err(ApiError::invalid(
            "The selected analysis has no saved solution",
        )),
    }
}

/// The selected catalog entry owns this position, including an absent answer.
pub(crate) async fn saved_checker_move(
    pool: &PgPool,
    position: &Position,
) -> anyhow::Result<Option<(String, String, Option<String>)>> {
    let mut cards = position_cards(pool, &[position.id], None, false)
        .await
        .map_err(|error| anyhow::anyhow!("{}", error.1.error))?;
    let Some(card) = cards.remove(&position.id).flatten() else {
        return Ok(None);
    };
    let best = card
        .solution
        .decision
        .as_ref()
        .map(|d| d.best.clone())
        .or_else(|| {
            card.solution
                .candidates
                .first()
                .filter(|c| c.rank == 1)
                .map(|c| c.notation.clone())
        });
    let depth = card
        .solution
        .imported
        .as_ref()
        .and_then(|record| record.depth.clone())
        .or_else(|| card.solution.plies.map(|plies| format!("{plies}-ply")));
    Ok(best
        .filter(|best| after_play(position, best).is_some())
        .map(|best| (best, card.solution.engine, depth)))
}

async fn from_position(pool: &PgPool, id: Uuid, request: &AddCards) -> ApiResult<Vec<CardContent>> {
    let position = store::get_position(pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    if request.match_id.is_some_and(|m| m != position.match_uuid) {
        return Err(ApiError::invalid("match_id does not match this position"));
    }
    if request.game_no.is_some_and(|n| n != position.game_no) {
        return Err(ApiError::invalid("game_no does not match this position"));
    }
    if request.player.is_some_and(|p| p != position.player) {
        return Ok(vec![]);
    }
    let selection = analysis_catalog::resolve_selection(
        pool,
        position.match_uuid,
        request.analysis_id.as_deref(),
    )
    .await?
    .ok_or_else(|| ApiError::invalid("The selected analysis has no saved solution"))?;
    let mut cards = position_cards(pool, &[id], Some(&selection.id()), false).await?;
    let card = cards.remove(&id).flatten().ok_or_else(|| {
        ApiError::invalid("The selected analysis has no supported solution for this position")
    })?;
    if request.min_loss.is_some_and(|loss| {
        card.solution
            .decision
            .as_ref()
            .is_none_or(|d| d.equity_loss < loss)
    }) {
        return Ok(vec![]);
    }
    Ok(vec![card])
}

pub(crate) async fn saved_position_cards(
    pool: &PgPool,
    ids: &[Uuid],
) -> ApiResult<std::collections::HashMap<Uuid, Option<CardContent>>> {
    position_cards(pool, ids, None, true).await
}

/// Card content of the occurrences `$1` from analysis `$2`, or from each
/// match's effective selection, limited to saved positions when `$3`.
/// OFFSET 0 keeps the selection and catalog lookups per match; joined
/// directly, PostgreSQL may build the analysis catalog of the whole library.
pub(crate) fn position_cards_sql() -> String {
    format!(
        "SELECT to_jsonb(p)||jsonb_build_object('cube_enabled',g.cube_enabled) AS position,m.white,m.black,m.format,c.analysis_id,c.engine,j.id AS job_id,j.kind AS job_kind,j.plies,j.result,
                ma.data AS decision,ia.id AS imported_id,ia.data AS imported,to_jsonb(e) AS event
         FROM positions p JOIN matches m ON m.id=p.match_uuid
         LEFT JOIN match_games g ON g.match_uuid=p.match_uuid AND g.game_no=p.game_no
         LEFT JOIN LATERAL (SELECT effective.analysis_id FROM effective_analysis_selections effective WHERE effective.match_id=m.id OFFSET 0) selected ON true
         LEFT JOIN LATERAL (SELECT catalog.analysis_id,catalog.engine,catalog.job_id,catalog.source FROM analysis_catalog_sources catalog WHERE catalog.match_id=m.id AND catalog.analysis_id=coalesce($2,selected.analysis_id) OFFSET 0) c ON true
         LEFT JOIN jobs j ON j.id=c.job_id AND (j.kind='match' AND (j.game_no IS NULL OR j.game_no=p.game_no) OR j.kind='analysis' AND j.position_uuid=p.id)
         LEFT JOIN LATERAL (SELECT data FROM move_analyses WHERE job_id=j.id AND position_uuid=p.id AND game_no=p.game_no AND kind='checker' ORDER BY id DESC LIMIT 1) ma ON true
         LEFT JOIN LATERAL (SELECT id,data,source_move_no FROM primary_imported_analyses WHERE match_uuid=m.id AND position_uuid=p.id AND kind='checker' ORDER BY source_ordinal LIMIT 1) ia ON c.source='imported'
         LEFT JOIN match_moves e ON e.match_uuid=m.id AND e.game_no=p.game_no AND e.move_no=ia.source_move_no AND e.position_uuid=p.id
         WHERE p.id=ANY($1) AND (NOT $3 OR EXISTS(SELECT 1 FROM saved_positions s WHERE s.position_uuid=p.id)) AND {}",
        crate::position_browser::SAVED_VISIBLE,
    )
}

async fn position_cards(
    pool: &PgPool,
    ids: &[Uuid],
    requested: Option<&str>,
    saved_only: bool,
) -> ApiResult<std::collections::HashMap<Uuid, Option<CardContent>>> {
    // Catalog only legacy hosts touched by this bounded occurrence selection.
    let legacy: Vec<Uuid> = sqlx::query_scalar("SELECT DISTINCT m.id FROM positions p JOIN matches m ON m.id=p.match_uuid WHERE p.id=ANY($1) AND (NOT m.moves_cataloged OR NOT m.imported_analysis_loaded)")
        .bind(ids).fetch_all(pool).await.map_err(anyhow::Error::from)?;
    for match_id in legacy {
        crate::move_catalog::ensure_catalog(pool, match_id).await?;
        crate::imported_analysis::ensure_catalog(pool, match_id).await?;
    }
    #[derive(FromRow)]
    struct Row {
        position: sqlx::types::Json<Position>,
        white: String,
        black: String,
        format: MatchFormat,
        analysis_id: Option<String>,
        job_id: Option<Uuid>,
        job_kind: Option<String>,
        engine: Option<String>,
        plies: Option<i32>,
        result: Option<serde_json::Value>,
        decision: Option<serde_json::Value>,
        imported_id: Option<Uuid>,
        imported: Option<serde_json::Value>,
        event: Option<sqlx::types::Json<crate::move_catalog::StoredMove>>,
    }
    let rows = sqlx::query_as::<_, Row>(&position_cards_sql())
        .bind(ids)
        .bind(requested)
        .bind(saved_only)
        .fetch_all(pool)
        .await
        .map_err(anyhow::Error::from)?;
    let mut output = std::collections::HashMap::with_capacity(rows.len());
    for row in rows {
        let position = row.position.0;
        let id = position.id;
        let players = [row.white, row.black];
        let card = if let (Some(imported_id), Some(value), Some(event)) =
            (row.imported_id, row.imported, row.event)
        {
            imported_card(
                imported_id,
                value,
                event.0,
                Some(position.ply),
                players,
                row.format,
                position.cube_enabled,
            )
        } else if let (Some(job_id), Some(kind), Some(plies), Some(engine)) =
            (row.job_id, row.job_kind, row.plies, row.engine)
        {
            let solution = if kind == "analysis" {
                row.result
                    .and_then(|v| serde_json::from_value::<Analysis>(v).ok())
                    .map(|a| CardSolution {
                        engine: a.engine,
                        plies: Some(a.plies),
                        candidates: a.candidates,
                        decision: None,
                        imported: None,
                    })
            } else {
                row.decision
                    .and_then(|v| serde_json::from_value::<Decision>(v).ok())
                    .map(|decision| CardSolution {
                        engine,
                        plies: Some(plies),
                        candidates: vec![],
                        decision: Some(decision),
                        imported: None,
                    })
            };
            solution
                .filter(has_usable_checker_solution)
                .map(|solution| CardContent {
                    kind: "checker".into(),
                    prompt: prompt("checker").into(),
                    played: solution
                        .decision
                        .as_ref()
                        .map(|d| d.played.clone())
                        .unwrap_or_else(|| position.notation.clone()),
                    source: CardSource {
                        kind,
                        job_id: Some(job_id),
                        analysis_id: row.analysis_id,
                        imported_analysis_id: None,
                        match_id: position.match_uuid,
                        position_id: Some(id),
                        game_no: position.game_no,
                        decision_no: solution.decision.as_ref().map(|d| d.decision_no),
                    },
                    solution,
                    position,
                    players,
                })
        } else {
            None
        };
        output.insert(id, card);
    }
    Ok(output)
}

fn imported_card(
    id: Uuid,
    value: serde_json::Value,
    event: crate::move_catalog::StoredMove,
    checker_ply: Option<i32>,
    players: [String; 2],
    format: MatchFormat,
    cube_enabled: Option<bool>,
) -> Option<CardContent> {
    let record =
        serde_json::from_value::<crate::imported_analysis::ImportedAnalysisData>(value).ok()?;
    let source = crate::recurring_mistakes::imported_projection::source_move(&event, checker_ply)?;
    let projected = crate::recurring_mistakes::imported_projection::project(
        &record,
        &source,
        event.match_uuid,
        event.position_uuid,
        format,
        cube_enabled,
    )?;
    let kind = if record.kind == "checker" {
        "checker"
    } else if matches!(projected.decision.played.as_str(), "take" | "drop") {
        "cube_response"
    } else {
        "cube_offer"
    };
    Some(CardContent {
        kind: kind.into(),
        prompt: prompt(kind).into(),
        played: projected.decision.played.clone(),
        source: CardSource {
            kind: "imported".into(),
            job_id: None,
            analysis_id: Some(format!("imported:{}", event.match_uuid)),
            imported_analysis_id: Some(id),
            match_id: event.match_uuid,
            position_id: event.position_uuid,
            game_no: event.game_no,
            decision_no: Some(event.move_no),
        },
        solution: CardSolution {
            engine: record.engine.clone(),
            plies: projected.depth_plies,
            candidates: vec![],
            decision: Some(projected.decision),
            imported: Some(record),
        },
        position: projected.position,
        players,
    })
}

async fn imported_match(
    pool: &PgPool,
    match_id: Uuid,
    request: &AddCards,
) -> ApiResult<Vec<CardContent>> {
    let cards =
        imported_decision_cards(pool, match_id, request.game_no, request.player, None).await?;
    let mut output = Vec::new();
    for (_, card) in cards {
        let card = card.map_err(ApiError::invalid)?;
        if request.min_loss.is_some_and(|loss| {
            card.solution
                .decision
                .as_ref()
                .is_none_or(|d| d.equity_loss < loss)
        }) {
            continue;
        }
        output.push(card);
        if output.len() > MAX_CARDS as usize {
            return Err(ApiError::invalid(
                "Batch exceeds 1000 cards; narrow the player or minimum-loss filter",
            ));
        }
    }
    if output.is_empty() && request.min_loss.is_none() {
        return Err(ApiError::invalid(
            "The selected imported analysis has no supported solutions in this scope",
        ));
    }
    Ok(output)
}

/// The cards of a match's primary imported analysis, in source order, keyed
/// by imported analysis row. A row without a supported card has an error
/// message instead.
async fn imported_decision_cards(
    pool: &PgPool,
    match_id: Uuid,
    game_no: Option<i32>,
    player: Option<i32>,
    ids: Option<&[Uuid]>,
) -> ApiResult<Vec<(Uuid, Result<CardContent, String>)>> {
    crate::move_catalog::ensure_catalog(pool, match_id).await?;
    #[derive(FromRow)]
    struct Row {
        cube_enabled: Option<bool>,
        id: Uuid,
        data: serde_json::Value,
        event: Option<sqlx::types::Json<crate::move_catalog::StoredMove>>,
        checker_ply: Option<i32>,
        white: String,
        black: String,
        format: MatchFormat,
    }
    // Starting at the requested IDs bounds primary ranking to their source moves.
    // Filtering the competing evaluations by ID before ranking would make a
    // secondary evaluation look primary.
    let rows = sqlx::query_as::<_, Row>(
        "WITH selected AS (
             SELECT ia.* FROM primary_imported_analyses ia WHERE $4::uuid[] IS NULL
             UNION ALL
             SELECT winner.* FROM imported_analyses requested
             JOIN LATERAL (
                 SELECT ia.* FROM primary_imported_analyses ia
                 WHERE ia.match_uuid=requested.match_uuid AND ia.game_no=requested.game_no
                   AND ia.source_move_no=requested.source_move_no
             ) winner ON winner.id=requested.id
             WHERE requested.id=ANY($4)
         )
         SELECT g.cube_enabled,ia.id,ia.data,to_jsonb(e) AS event,p.ply AS checker_ply,
                m.white,m.black,m.format
         FROM selected ia JOIN matches m ON m.id=ia.match_uuid
         LEFT JOIN match_moves e ON e.match_uuid=ia.match_uuid AND e.game_no=ia.game_no
             AND e.move_no=ia.source_move_no AND e.position_uuid IS NOT DISTINCT FROM ia.position_uuid
         LEFT JOIN positions p ON p.id=e.position_uuid
         JOIN match_games g ON g.match_uuid=ia.match_uuid AND g.game_no=ia.game_no
         WHERE m.id=$1 AND NOT m.collection_only AND (g.cube_enabled OR ia.kind='checker')
           AND ia.kind IN ('checker','cube') AND ($2::integer IS NULL OR ia.game_no=$2)
           AND ($3::integer IS NULL OR e.player=$3)
         ORDER BY ia.game_no,ia.source_move_no,ia.source_ordinal LIMIT 200001",
    )
    .bind(match_id)
    .bind(game_no)
    .bind(player)
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    if rows.len() > 200000 {
        return Err(ApiError::invalid(
            "Analysis exceeds supported decision count",
        ));
    }
    Ok(rows
        .into_iter()
        .map(|row| {
            let card = match row.event {
                None => Err("The selected imported analysis lacks its original decision context".to_owned()),
                Some(event) => imported_card(row.id, row.data, event.0, row.checker_ply, [row.white, row.black], row.format, row.cube_enabled)
                    .ok_or_else(|| "The selected imported analysis contains an unsupported solution; select supported positions individually".to_owned()),
            };
            (row.id, card)
        })
        .collect())
}

async fn from_match(pool: &PgPool, id: Uuid, request: &AddCards) -> ApiResult<Vec<CardContent>> {
    let player = request.player;
    let min_loss = request.min_loss.unwrap_or(0.0);
    let source = MatchCardSource::load(pool, id).await?;
    let cards = source
        .cards(pool, request.game_no, move |d| {
            player.is_none_or(|p| p == d.player) && d.equity_loss >= min_loss
        })
        .await?;
    let mut output = Vec::new();
    for (_, card) in cards {
        if output.len() >= MAX_CARDS as usize {
            return Err(ApiError::invalid(
                "Batch exceeds 1000 cards; narrow the player or minimum-loss filter",
            ));
        }
        output.push(card.map_err(ApiError::invalid)?);
    }
    Ok(output)
}

/// One decision of a saved match analysis as a card, with the board and match
/// state of the original game.
#[allow(clippy::too_many_arguments)]
fn decision_card(
    position_uuid: Option<Uuid>,
    game_no: i32,
    match_id: Uuid,
    position_id: &str,
    match_state_id: &str,
    players: [String; 2],
    job_id: Uuid,
    engine: String,
    plies: i32,
    d: Decision,
) -> anyhow::Result<CardContent> {
    let kind = if d.kind == "checker" {
        "checker"
    } else if matches!(d.played.as_str(), "take" | "drop") {
        "cube_response"
    } else {
        "cube_offer"
    };
    let board = decode_board(position_id, match_state_id)?;
    let original_player = ids::on_roll(match_state_id).context("Invalid GNU ID length")?;
    ensure!(
        original_player == d.player,
        "Decision player differs from source GNU ID"
    );
    let position = Position {
        cube_enabled: None,
        id: position_uuid.unwrap_or(Uuid::nil()),
        match_uuid: match_id,
        ordinal: d.checker_no.unwrap_or(d.decision_no),
        game_no,
        ply: d.checker_no.unwrap_or(0),
        player: d.player,
        dice: if kind == "checker" {
            d.dice.context("Checker decision has no dice")?.to_vec()
        } else {
            vec![0, 0]
        },
        board,
        position_id: position_id.to_owned(),
        gnubg_match_id: match_state_id.to_owned(),
        notation: String::new(),
    };
    let card = match_card(kind, position, players, job_id, engine, plies, d);
    ensure!(
        if kind == "checker" {
            has_usable_checker_solution(&card.solution)
        } else {
            card.solution
                .decision
                .as_ref()
                .is_some_and(|d| crate::match_analysis::evaluated_equities(d).is_some())
        },
        "The selected analysis contains an unavailable decision; select supported positions individually"
    );
    Ok(card)
}
pub(super) fn match_card(
    kind: &str,
    position: Position,
    players: [String; 2],
    job_id: Uuid,
    engine: String,
    plies: i32,
    decision: Decision,
) -> CardContent {
    CardContent {
        kind: kind.into(),
        prompt: prompt(kind).into(),
        players,
        source: CardSource {
            kind: "match".into(),
            job_id: Some(job_id),
            analysis_id: Some(format!("job:{job_id}")),
            imported_analysis_id: None,
            match_id: position.match_uuid,
            position_id: (!position.id.is_nil()).then_some(position.id),
            game_no: position.game_no,
            decision_no: Some(decision.decision_no),
        },
        played: decision.played.clone(),
        solution: CardSolution {
            engine,
            plies: Some(plies),
            imported: None,
            candidates: vec![],
            decision: Some(decision),
        },
        position,
    }
}

fn prompt(kind: &str) -> &'static str {
    match kind {
        "cube_offer" => "Would you double or play on?",
        "cube_response" => "The cube has been offered. Take or pass?",
        _ => "What is the best checker play?",
    }
}

pub(super) async fn answer_on_connection(
    connection: &mut sqlx::PgConnection,
    id: Uuid,
    c: &CardContent,
) -> anyhow::Result<CardAnswer> {
    let annotations: Vec<(Vec<String>, String)> = sqlx::query_as(
        &format!("(SELECT a.tags,a.comment FROM learning_cards c JOIN annotations a ON {} WHERE c.id=$1 ORDER BY a.target_kind,a.id)
                  UNION ALL SELECT tags,'' FROM learning_cards WHERE id=$1", super::search::NOTE_SCOPE),
    )
    .bind(id)
    .fetch_all(connection)
    .await?;
    let mut tags = BTreeSet::new();
    let mut notes = Vec::new();
    for (annotation_tags, comment) in annotations {
        tags.extend(annotation_tags);
        if !comment.is_empty() {
            notes.push(comment);
        }
    }
    Ok(CardAnswer {
        solution: Some(c.solution.clone()),
        notes: notes.join("\n\n"),
        tags: tags.into_iter().collect(),
        played: c.played.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decode_absolute_board_for_both_players_and_bar() {
        let input = "(;GM[6]MI[length:7]AE[a:y]AW[a][a][f]AB[s][u][x]PL[B]CV[2]CO[b];B[31uxst])";
        let parsed = crate::import::parse_sgf(input).unwrap();
        for p in parsed.positions {
            assert_eq!(decode_board(&p.position_id, &p.match_id).unwrap(), p.board);
        }
        let p = &crate::import::parse_sgf("(;GM[6];W[31hefe])")
            .unwrap()
            .positions[0];
        assert_eq!(decode_board(&p.position_id, &p.match_id).unwrap(), p.board);
        assert!(decode_board("AAAAAAAAAAAAAA", "AAAAAAAAAAAA").is_ok());
        assert!(decode_board("/////////////w", "AAAAAAAAAAAA").is_err());
    }
}
