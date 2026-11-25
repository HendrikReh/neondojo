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
    let source = library::normalize_native_setup_turn(sgf.to_owned())?;
    let engine = session.analysis_engine(path)?;
    let numbers = if session.match_length == 0 {
        vec![session.game_no]
    } else {
        (1..=crate::match_analysis::game_count(&source)?).collect()
    };
    let mut games = Vec::new();
    let imported = crate::import::sgf::parse_native_sgf_source_moves(&source)?.0;
    for game in numbers {
        if session.match_length == 0
            && !imported
                .positions
                .iter()
                .any(|position| position.game_no == game)
        {
            games.push(crate::match_analysis::AnalyzedGame {
                cube_enabled: imported
                    .games
                    .iter()
                    .find(|g| g.game_no == game)
                    .and_then(|g| g.cube_enabled)
                    .context("Missing game rules")?,
                engine: session.analysis_version().into(),
                game_no: game,
                decisions: Vec::new(),
                statistics: None,
            });
            continue;
        }
        games.push(
            engine
                .analyze_game(&source, game, session.analysis_plies)
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
