//! Immutable report checkpoints outlive later games in the same native session.
use super::{PlayPhase, PlaySession, feedback, library};
use anyhow::{Context, Result};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) fn money_finished(session: &PlaySession) -> bool {
    session.match_length == 0 && matches!(session.phase, PlayPhase::GameOver | PlayPhase::MatchOver)
}
pub(super) async fn build(session: &mut PlaySession, sgf: &str, path: &str) -> Result<()> {
    if session.match_forfeited && sgf.is_empty() {
        session.review = Some(feedback::build_review(session, Vec::new()));
        return Ok(());
    }
    let money_game = (session.match_length == 0).then_some(session.game_no);
    let sgf = sgf.to_owned();
    let (source, report_games) = tokio::task::spawn_blocking(move || {
        let source = library::normalize_native_setup_turn(sgf)?;
        // Native event replay also validates resignations and pending cube offers.
        let (imported, _) = crate::import::sgf::parse_native_sgf_source_moves(&source)?;
        let games = match money_game {
            Some(number) => vec![
                imported
                    .games
                    .into_iter()
                    .find(|game| game.game_no == number)
                    .context("Missing game rules")?,
            ],
            None => imported.games,
        };
        anyhow::Ok((source, games))
    })
    .await
    .context("Play report SGF preparation worker failed")??;
    let engine = session.analysis_engine(path)?;
    let mut games = Vec::new();
    for game in report_games {
        if money_game.is_some() && game.position_count == 0 {
            games.push(crate::match_analysis::AnalyzedGame {
                cube_enabled: game.cube_enabled.context("Missing game rules")?,
                engine: session.analysis_version().into(),
                game_no: game.game_no,
                decisions: Vec::new(),
                statistics: None,
            });
            continue;
        }
        games.push(
            engine
                .analyze_game(&source, game.game_no, session.analysis_plies)
                .await?,
        );
    }
    let mut reviewed = session.clone();
    if session.match_length == 0 {
        reviewed
            .decisions
            .retain(|decision| decision.game_no == session.game_no);
        reviewed.learning_used =
            session.game_learning_used || (session.game_no == 1 && session.learning_used);
    }
    session.review = Some(feedback::build_review(&reviewed, games));
    Ok(())
}
pub(super) async fn archive(
    tx: &mut Transaction<'_, Postgres>,
    session: &PlaySession,
    sgf: &str,
) -> Result<(Uuid, i64)> {
    let game = if session.match_length == 0 {
        session.game_no
    } else {
        0
    };
    let id = if game == 0
        || (matches!(session.phase, PlayPhase::MatchOver) && !session.money_session_ended)
    {
        session.id
    } else {
        Uuid::new_v4()
    };
    let mut snapshot = session.clone();
    snapshot.phase = PlayPhase::MatchOver;
    snapshot.study_report_id = Some(id);
    snapshot.study_report_version = Some(session.version);
    snapshot.review_game_no = (game > 0).then_some(game);
    if game > 0 {
        snapshot
            .decisions
            .retain(|decision| decision.game_no == game);
    }
    sqlx::query("INSERT INTO study_report_snapshots(id,session_id,game_no,library_match_id,state,sgf) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(session_id,game_no) DO NOTHING")
        .bind(id).bind(session.id).bind(game).bind(session.library_match_id).bind(sqlx::types::Json(snapshot)).bind(sgf).execute(&mut **tx).await?;
    Ok(sqlx::query_as("SELECT id,(state->>'version')::bigint FROM study_report_snapshots WHERE session_id=$1 AND game_no=$2").bind(session.id).bind(game).fetch_one(&mut **tx).await?)
}
