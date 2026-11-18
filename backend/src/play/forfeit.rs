//! A match concession ends point play without adding an engine move or game result.
use super::{PlayActionRequest, PlayPhase, PlaySession, Saved, pause, persist_session};
use crate::{
    api::{ApiError, ApiResult, AppState},
    import::sgf::MATCH_FORFEIT_PROPERTY,
};
use axum::Json;

pub(super) async fn finish(
    state: &AppState,
    saved: Saved,
    request: &PlayActionRequest,
) -> ApiResult<Json<PlaySession>> {
    if request.move_index.is_some()
        || request.resign_level.is_some()
        || request.decision_id.is_some()
    {
        return Err(ApiError::invalid(
            "Forfeiting a match only requires a session version",
        ));
    }
    let mut session = saved.state.0;
    concede(&mut session)?;
    persist_session(
        state,
        &session,
        &saved.sgf,
        &saved.native_id,
        request.version,
        true,
    )
    .await
}

fn concede(session: &mut PlaySession) -> ApiResult<()> {
    pause::require_active(session)?;
    if session.match_length == 0 || matches!(session.phase, PlayPhase::MatchOver) {
        return Err(ApiError::invalid(
            "Only an unfinished point match can be forfeited",
        ));
    }
    let winner = 1 - session.human_player;
    session.scores[winner as usize] = session.match_length;
    session.winner = Some(winner);
    session.match_forfeited = true;
    session.phase = PlayPhase::MatchOver;
    session.version += 1;
    session.can_double = false;
    session.legal_moves.clear();
    session.pending_decision = None;
    session.opening = None;
    Ok(())
}

/// Record the conceding color on the first root of the exported or archived SGF.
/// The native checkpoint and its per-game scores are left intact.
pub(super) fn annotate(session: &PlaySession, mut sgf: String) -> String {
    if session.match_forfeited
        && let Some(root) = sgf.find(';')
    {
        let player = if session.human_player == 0 { "W" } else { "B" };
        sgf.insert_str(root + 1, &format!("{MATCH_FORFEIT_PROPERTY}[{player}]"));
    }
    sgf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{databases::source_metadata, import::parse_sgf};
    use serde_json::json;

    const SOURCE: &str = "(;GM[6]MI[length:7][game:0][ws:0][bs:0]RE[B+2R];W[31hefe])
(;GM[6]MI[length:7][game:1][ws:0][bs:2];W[31hefe])";

    fn session(human: i32) -> PlaySession {
        let id = uuid::Uuid::new_v4();
        let mut session = serde_json::from_value::<PlaySession>(json!({
            "id": id, "version": 4, "human_player": human, "human_name": "Human",
            "match_length": 7, "bearoff_direction": "right",
            "engine": {"id": "gnubg", "version": "test", "plies": 1},
            "position": {
                "id": id, "match_uuid": id, "ordinal": 0, "game_no": 2, "ply": 0,
                "player": human, "dice": [3, 1], "board": vec![0; 26],
                "position_id": "", "gnubg_match_id": "", "notation": ""
            },
            "phase": "move", "can_double": true,
            "legal_moves": [{"notation": "8/5 6/5", "steps": []}],
            "history": [{"player": 0, "dice": [3, 1], "notation": "8/5 6/5"}],
            "scores": [0, 2], "game_no": 2, "winner": null
        }))
        .unwrap();
        assert!(!session.match_forfeited);
        session.pending_decision = Some(super::super::feedback::empty_decision(Vec::new()));
        session
    }

    #[test]
    fn forfeit_preserves_play_and_records_a_match_win_for_either_bot_color() {
        for (human, expected_scores) in [(0, [0, 7]), (1, [7, 2])] {
            let mut session = session(human);
            let position = serde_json::to_value(&session.position).unwrap();
            let history = serde_json::to_value(&session.history).unwrap();
            assert!(concede(&mut session).is_ok());

            assert_eq!(session.scores, expected_scores);
            assert_eq!(session.winner, Some(1 - human));
            assert_eq!(session.version, 5);
            assert!(matches!(session.phase, PlayPhase::MatchOver));
            assert!(session.match_forfeited);
            assert!(!session.can_double);
            assert!(session.legal_moves.is_empty());
            assert!(session.pending_decision.is_none());
            assert!(session.decisions.is_empty());
            assert_eq!(serde_json::to_value(&session.position).unwrap(), position);
            assert_eq!(serde_json::to_value(&session.history).unwrap(), history);

            let archived = annotate(&session, SOURCE.into());
            let metadata = source_metadata(&archived);
            assert_eq!(metadata.result_state, "completed");
            assert_eq!(metadata.white_score, Some(expected_scores[0]));
            assert_eq!(metadata.black_score, Some(expected_scores[1]));
            let imported = parse_sgf(&archived).unwrap();
            let original = parse_sgf(SOURCE).unwrap();
            assert_eq!(imported.positions.len(), original.positions.len());
            assert_eq!(
                imported.positions[1].match_id,
                original.positions[1].match_id
            );
            assert_eq!(imported.games[0].result, original.games[0].result);
            assert!(imported.games[0].result.is_some());
            assert!(imported.games[1].result.is_none());
        }
    }

    #[test]
    fn forfeit_between_games_does_not_rewrite_the_game_winner() {
        let mut session = session(1);
        session.phase = PlayPhase::GameOver;
        session.winner = Some(1);
        assert!(concede(&mut session).is_ok());

        let played_game = SOURCE.lines().next().unwrap();
        let archived = annotate(&session, played_game.into());
        assert_eq!(source_metadata(&archived).white_score, Some(7));
        let game = parse_sgf(&archived).unwrap().games.remove(0);
        assert_eq!(game.result.unwrap().winner, 1);
        assert_eq!(session.winner, Some(0));
    }

    #[test]
    fn rejects_paused_money_and_completed_sessions_without_changes() {
        for (paused, length, phase) in [
            (true, 7, PlayPhase::Move),
            (false, 0, PlayPhase::Move),
            (false, 7, PlayPhase::MatchOver),
        ] {
            let mut session = session(0);
            session.paused = paused;
            session.match_length = length;
            session.phase = phase;
            let before = serde_json::to_value(&session).unwrap();
            assert!(concede(&mut session).is_err());
            assert_eq!(serde_json::to_value(&session).unwrap(), before);
        }
    }
}
