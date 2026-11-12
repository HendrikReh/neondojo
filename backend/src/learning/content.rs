use super::*;
use crate::{engine::Analysis, move_assessments::after_play, store};
use anyhow::{Context, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use std::collections::BTreeSet;

pub(super) async fn from_source(pool: &PgPool, request: &AddCards) -> ApiResult<Vec<CardContent>> {
    if let Some(id) = request.position_id {
        return from_position(pool, id, request).await;
    }
    from_match(pool, request.job_id.unwrap(), request).await
}
async fn names(pool: &PgPool, id: Uuid) -> anyhow::Result<[String; 2]> {
    let (white, black): (String, String) =
        sqlx::query_as("SELECT white,black FROM matches WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await?;
    Ok([white, black])
}

async fn individual_hint(
    pool: &PgPool,
    id: Uuid,
) -> anyhow::Result<Option<(Uuid, serde_json::Value)>> {
    Ok(sqlx::query_as("SELECT id,result FROM selected_engine_jobs WHERE position_uuid=$1 AND kind='analysis' AND status='completed' AND result IS NOT NULL ORDER BY plies DESC,created_at DESC,id DESC LIMIT 1")
        .bind(id).fetch_optional(pool).await?)
}

#[derive(FromRow)]
struct MatchCheckerAnalysis {
    id: Uuid,
    plies: i32,
    data: serde_json::Value,
    engine: String,
}

async fn match_checker_analysis(
    pool: &PgPool,
    id: Uuid,
) -> anyhow::Result<Option<MatchCheckerAnalysis>> {
    Ok(sqlx::query_as("SELECT j.id,j.plies,m.data,
            CASE WHEN jsonb_typeof(j.result->'engine')='string' THEN j.result->>'engine' ELSE 'GNU Backgammon' END AS engine
        FROM move_analyses m JOIN selected_engine_jobs j ON j.id=m.job_id WHERE m.position_uuid=$1 AND m.kind='checker' AND j.status='completed' ORDER BY j.plies DESC,j.created_at DESC,j.id DESC LIMIT 1")
        .bind(id).fetch_optional(pool).await?)
}

/// Practice can continue to another source when the preferred stored analysis has no answer.
pub(crate) async fn saved_checker_move(
    pool: &PgPool,
    position: &Position,
) -> anyhow::Result<Option<(String, String, i32)>> {
    if let Some((_, result)) = individual_hint(pool, position.id).await?
        && let Ok(analysis) = serde_json::from_value::<Analysis>(result)
        && let Some(best) = analysis.candidates.into_iter().next()
        && best.rank == 1
        && after_play(position, &best.notation).is_some()
    {
        return Ok(Some((best.notation, analysis.engine, analysis.plies)));
    }
    if let Some(row) = match_checker_analysis(pool, position.id).await?
        && let Ok(decision) = serde_json::from_value::<Decision>(row.data)
        && decision.kind == "checker"
        && decision.player == position.player
        && matches!(decision.evaluation_status.as_str(), "evaluated" | "forced")
        && after_play(position, &decision.best).is_some()
    {
        return Ok(Some((decision.best, row.engine, row.plies)));
    }
    Ok(None)
}

async fn from_position(pool: &PgPool, id: Uuid, request: &AddCards) -> ApiResult<Vec<CardContent>> {
    let position = store::get_position(pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    if request.game_no.is_some_and(|n| n != position.game_no) {
        return Err(ApiError::invalid("game_no does not match this position"));
    }
    if request.player.is_some_and(|p| p != position.player) {
        return Ok(vec![]);
    }
    let players = names(pool, position.match_uuid).await?;
    if let Some((job_id, result)) = individual_hint(pool, id).await? {
        let analysis: Analysis = serde_json::from_value(result).map_err(anyhow::Error::from)?;
        if analysis.candidates.is_empty() {
            return Err(ApiError::invalid("Completed analysis has no candidates"));
        }
        return Ok(vec![CardContent {
            kind: "checker".into(),
            prompt: prompt("checker").into(),
            played: position.notation.clone(),
            source: CardSource {
                kind: "analysis".into(),
                job_id,
                match_id: position.match_uuid,
                position_id: Some(position.id),
                game_no: position.game_no,
                decision_no: None,
            },
            solution: CardSolution {
                engine: analysis.engine,
                plies: analysis.plies,
                candidates: analysis.candidates,
                decision: None,
            },
            position,
            players,
        }]);
    }
    if let Some(analysis) = match_checker_analysis(pool, id).await? {
        let decision: Decision =
            serde_json::from_value(analysis.data).map_err(anyhow::Error::from)?;
        if request
            .min_loss
            .is_some_and(|loss| decision.equity_loss < loss)
        {
            return Ok(vec![]);
        }
        return Ok(vec![CardContent {
            kind: "checker".into(),
            prompt: prompt("checker").into(),
            played: decision.played.clone(),
            source: CardSource {
                kind: "match".into(),
                job_id: analysis.id,
                match_id: position.match_uuid,
                position_id: Some(position.id),
                game_no: position.game_no,
                decision_no: Some(decision.decision_no),
            },
            solution: CardSolution {
                engine: analysis.engine,
                plies: analysis.plies,
                candidates: vec![],
                decision: Some(decision),
            },
            position,
            players,
        }]);
    }
    Err(ApiError::invalid(
        "Analyze this position or complete its game or match analysis before adding it",
    ))
}

#[derive(FromRow)]
struct DecisionRow {
    position_uuid: Option<Uuid>,
    game_no: i32,
    data: serde_json::Value,
}
async fn from_match(pool: &PgPool, id: Uuid, request: &AddCards) -> ApiResult<Vec<CardContent>> {
    let job = store::get_job(pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    if job.kind != "match" || job.status != "completed" {
        return Err(ApiError::invalid(
            "job_id must refer to a completed match analysis",
        ));
    }
    let match_id = job.match_uuid.context("Match job lacks match UUID")?;
    if !store::get_match(pool, match_id)
        .await?
        .is_some_and(|m| m.source_available)
    {
        return Err(ApiError::invalid(
            "The source match was removed; add its saved positions instead",
        ));
    }
    if request
        .game_no
        .is_some_and(|n| job.game_no.is_some_and(|scope| scope != n))
    {
        return Err(ApiError::invalid("game_no does not match this job's scope"));
    }
    if let Some(game_no) = request.game_no {
        let games = store::match_games(pool, match_id)
            .await?
            .ok_or_else(ApiError::missing)?;
        if !games.iter().any(|g| g.game_no == game_no) {
            return Err(ApiError::invalid("game_no is outside this match"));
        }
    }
    let (sgf, white, black): (String, String, String) =
        sqlx::query_as("SELECT sgf,white,black FROM matches WHERE id=$1")
            .bind(match_id)
            .fetch_one(pool)
            .await
            .map_err(anyhow::Error::from)?;
    let rows:Vec<DecisionRow>=sqlx::query_as("SELECT position_uuid,game_no,data FROM move_analyses WHERE job_id=$1 AND ($2::integer IS NULL OR game_no=$2) ORDER BY game_no,decision_no LIMIT 200001").bind(id).bind(request.game_no).fetch_all(pool).await.map_err(anyhow::Error::from)?;
    if rows.len() > 200000 {
        return Err(ApiError::invalid(
            "Analysis exceeds supported decision count",
        ));
    }
    let player = request.player;
    let min_loss = request.min_loss.unwrap_or(0.0);
    let engine = job
        .result
        .as_ref()
        .and_then(|r| r.get("engine"))
        .and_then(|e| e.as_str())
        .unwrap_or("GNU Backgammon")
        .to_owned();
    let contents = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<CardContent>> {
        let (_, contexts) = crate::import::sgf::parse_sgf_with_context(&sgf)?;
        let mut output = Vec::new();
        let mut last_game = 0;
        let mut action = 0;
        for row in rows {
            let d: Decision = serde_json::from_value(row.data)?;
            if row.game_no != last_game {
                last_game = row.game_no;
                action = 0;
            }
            let context = contexts
                .get(usize::try_from(row.game_no - 1)?)
                .context("Missing original game context")?;
            let (position_id, match_state_id) = context
                .decision_ids
                .get(action)
                .context("Missing original decision state")?;
            if d.played != "no double" {
                action += 1;
            }
            // Advance through every decision before filtering: cube responses have their own IDs.
            if player.is_some_and(|p| p != d.player) || d.equity_loss < min_loss {
                continue;
            }
            if output.len() >= MAX_CARDS as usize {
                anyhow::bail!("Batch exceeds 1000 cards; narrow the player or minimum-loss filter");
            }
            let kind = if d.kind == "checker" {
                "checker"
            } else if matches!(d.played.as_str(), "take" | "drop") {
                "cube_response"
            } else {
                "cube_offer"
            };
            let board = decode_board(position_id, match_state_id)?;
            let original_player = (STANDARD_NO_PAD.decode(match_state_id)?[0] >> 6) & 1;
            ensure!(
                i32::from(original_player) == d.player,
                "Decision player differs from source GNU ID"
            );
            let position = Position {
                id: row.position_uuid.unwrap_or(Uuid::nil()),
                match_uuid: match_id,
                ordinal: d.checker_no.unwrap_or(d.decision_no),
                game_no: row.game_no,
                ply: d.checker_no.unwrap_or(0),
                player: d.player,
                dice: if kind == "checker" {
                    d.dice.context("Checker decision has no dice")?.to_vec()
                } else {
                    vec![0, 0]
                },
                board,
                position_id: position_id.clone(),
                gnubg_match_id: match_state_id.clone(),
                notation: String::new(),
            };
            output.push(CardContent {
                kind: kind.into(),
                prompt: prompt(kind).into(),
                players: [white.clone(), black.clone()],
                source: CardSource {
                    kind: "match".into(),
                    job_id: id,
                    match_id,
                    position_id: row.position_uuid,
                    game_no: row.game_no,
                    decision_no: Some(d.decision_no),
                },
                solution: CardSolution {
                    engine: engine.clone(),
                    plies: job.plies,
                    candidates: vec![],
                    decision: Some(d.clone()),
                },
                played: d.played,
                position,
            });
        }
        Ok(output)
    })
    .await
    .map_err(anyhow::Error::from)?;
    match contents {
        Err(e) if e.to_string().starts_with("Batch exceeds") => {
            Err(ApiError::invalid(e.to_string()))
        }
        result => Ok(result?),
    }
}
fn prompt(kind: &str) -> &'static str {
    match kind {
        "cube_offer" => "Would you double or play on?",
        "cube_response" => "The cube has been offered. Take or pass?",
        _ => "What is the best checker play?",
    }
}

/// Inverse of GNUbg's unary opponent-then-player encoding, returned in absolute colors.
pub(crate) fn decode_board(position_id: &str, match_id: &str) -> anyhow::Result<Vec<i32>> {
    let key = STANDARD_NO_PAD.decode(position_id)?;
    let state = STANDARD_NO_PAD.decode(match_id)?;
    ensure!(key.len() == 10 && state.len() == 9, "Invalid GNU ID length");
    let player = i32::from((state[0] >> 6) & 1);
    let mut board = vec![0; 26];
    let mut bit = 0usize;
    for side in [1 - player, player] {
        let mut total = 0;
        for point in 1..=25 {
            let mut count = 0;
            while bit < 80 && (key[bit / 8] >> (bit % 8)) & 1 == 1 {
                count += 1;
                bit += 1;
            }
            ensure!(bit < 80, "Invalid unary GNU position ID");
            bit += 1;
            total += count;
            ensure!(total <= 15, "Invalid checker count in GNU position ID");
            let index = if side == 0 { point } else { 25 - point };
            if count > 0 {
                ensure!(board[index] == 0, "Opposing checkers share a point");
                board[index] = if side == 0 { count } else { -count };
            }
        }
    }
    ensure!(
        (bit..80).all(|i| key[i / 8] >> (i % 8) & 1 == 0),
        "Noncanonical GNU position ID"
    );
    Ok(board)
}

pub(super) async fn answer(pool: &PgPool, c: &CardContent) -> anyhow::Result<CardAnswer> {
    let mut connection = pool.acquire().await?;
    answer_on_connection(&mut connection, c).await
}

pub(super) async fn answer_on_connection(
    connection: &mut sqlx::PgConnection,
    c: &CardContent,
) -> anyhow::Result<CardAnswer> {
    // One statement reads the match and position annotations from the same fresh snapshot.
    // Export callers use their existing transaction connection while holding shared-card locks.
    let annotations: Vec<(Vec<String>, String)> = sqlx::query_as(
        "SELECT tags,comment FROM annotations WHERE match_uuid=$1 OR match_uuid=(SELECT match_uuid FROM positions WHERE id=$2) OR position_uuid=$2 ORDER BY target_kind",
    )
    .bind(c.source.match_id)
    .bind(c.source.position_id)
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
        solution: c.solution.clone(),
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
