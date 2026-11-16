//! Board facts that differ between two checker plays of the same roll.
//!
//! The facts are counted from the resulting boards alone. They describe what
//! a play does (blots left, shots allowed, points made, checkers hit); the
//! engine's equities remain the measure of which play is better.

use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use crate::move_assessments::after_checker_play;
use axum::{Json, Router, extract::rejection::JsonRejection, routing::post};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

/// One side's checkers seen from the player who just moved: that player
/// moves from point 24 toward point 1, the opponent in the other direction.
struct View {
    own: [i32; 25],
    opponent: [i32; 25],
    /// Opponent checkers on the bar; they enter on the mover's home board.
    opponent_bar: i32,
    own_bar: i32,
}

fn view(board: &[i32; 26], player: i32) -> View {
    let mut own = [0; 25];
    let mut opponent = [0; 25];
    for point in 1..=24 {
        let count = if player == 0 {
            board[point]
        } else {
            -board[25 - point]
        };
        own[point] = count.max(0);
        opponent[point] = (-count).max(0);
    }
    let (own_bar, opponent_bar) = if player == 0 {
        (board[25], -board[0])
    } else {
        (-board[0], board[25])
    };
    View {
        own,
        opponent,
        opponent_bar: opponent_bar.max(0),
        own_bar: own_bar.max(0),
    }
}

/// True if the opponent can hit a blot with some order and choice of checkers.
/// `checkers[0]` is the opponent's bar; index `p` is the mover's point `p`.
fn can_hit(own: &[i32; 25], checkers: &mut [i32; 25], dice: &[i32]) -> bool {
    let sources: Vec<usize> = if checkers[0] > 0 {
        vec![0]
    } else {
        (1..=24).filter(|point| checkers[*point] > 0).collect()
    };
    for (index, die) in dice.iter().enumerate() {
        // Equal dice lead to the same positions; try each value once.
        if dice[..index].contains(die) {
            continue;
        }
        let rest: Vec<i32> = dice
            .iter()
            .enumerate()
            .filter_map(|(other, value)| (other != index).then_some(*value))
            .collect();
        for &source in &sources {
            let Ok(step) = usize::try_from(*die) else {
                continue;
            };
            let target = source + step;
            // Bearing off cannot hit, and a made point blocks the move.
            if target > 24 || own[target] >= 2 {
                continue;
            }
            if own[target] == 1 {
                return true;
            }
            checkers[source] -= 1;
            checkers[target] += 1;
            let hit = !rest.is_empty() && can_hit(own, checkers, &rest);
            checkers[target] -= 1;
            checkers[source] += 1;
            if hit {
                return true;
            }
        }
    }
    false
}

/// Of the 36 equally likely rolls, how many let the opponent hit a blot.
fn hitting_rolls(view: &View) -> i32 {
    if !view.own[1..].contains(&1) {
        return 0;
    }
    let mut checkers = view.opponent;
    checkers[0] = view.opponent_bar;
    let mut rolls = 0;
    for first in 1..=6 {
        for second in first..=6 {
            let dice: &[i32] = if first == second {
                &[first; 4]
            } else {
                &[first, second]
            };
            if can_hit(&view.own, &mut checkers, dice) {
                rolls += if first == second { 1 } else { 2 };
            }
        }
    }
    rolls
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PlayFacts {
    /// The mover's single checkers on points 1 to 24, as point numbers.
    pub blots: Vec<i32>,
    /// Opponent rolls out of 36 that can hit one of those blots.
    pub hitting_rolls: i32,
    /// Points the mover holds with two or more checkers.
    pub points: Vec<i32>,
    /// Held points among the mover's home points 1 to 6.
    pub home_points: i32,
    /// Longest run of adjacent held points.
    pub longest_prime: i32,
    /// Opponent checkers on the bar after the play.
    pub opponent_on_bar: i32,
    pub pips: i32,
    pub opponent_pips: i32,
}

fn facts(board: &[i32; 26], player: i32) -> PlayFacts {
    let view = view(board, player);
    let held = |point: usize| view.own[point] >= 2;
    let mut longest = 0;
    let mut run = 0;
    for point in 1..=24 {
        run = if held(point) { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let point_numbers = |keep: &dyn Fn(usize) -> bool| -> Vec<i32> {
        (1..=24usize)
            .filter(|point| keep(*point))
            .map(|point| point as i32)
            .collect()
    };
    PlayFacts {
        blots: point_numbers(&|point| view.own[point] == 1),
        hitting_rolls: hitting_rolls(&view),
        points: point_numbers(&held),
        home_points: (1..=6).filter(|point| held(*point)).count() as i32,
        longest_prime: longest,
        opponent_on_bar: view.opponent_bar,
        pips: (1..=24).map(|p| p as i32 * view.own[p]).sum::<i32>() + 25 * view.own_bar,
        opponent_pips: (1..=24)
            .map(|p| (25 - p as i32) * view.opponent[p])
            .sum::<i32>()
            + 25 * view.opponent_bar,
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveExplanationRequest {
    /// Absolute-color board before the play, as in the game-plan request.
    #[schema(min_items = 26, max_items = 26)]
    pub board: [i32; 26],
    /// Mover: 0 White, 1 Black.
    #[schema(minimum = 0, maximum = 1)]
    pub player: i32,
    #[schema(min_items = 2, max_items = 2)]
    pub dice: [i32; 2],
    /// Checker notation of the two plays to compare.
    #[schema(max_length = 160)]
    pub best: String,
    #[schema(max_length = 160)]
    pub played: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MoveExplanation {
    pub best: PlayFacts,
    pub played: PlayFacts,
}

pub fn explain(request: &MoveExplanationRequest) -> Result<MoveExplanation, &'static str> {
    if !(0..=1).contains(&request.player) {
        return Err("player must be 0 (White) or 1 (Black)");
    }
    if request.dice.iter().any(|die| !(1..=6).contains(die)) {
        return Err("dice must be between 1 and 6");
    }
    crate::game_plan::classify(&request.board).map_err(|_| "Invalid board")?;
    let after = |notation: &str| {
        after_checker_play(request.board, request.player, request.dice, notation)
            .ok_or("A play is not legal checker notation for this board and roll")
    };
    Ok(MoveExplanation {
        best: facts(&after(&request.best)?, request.player),
        played: facts(&after(&request.played)?, request.player),
    })
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/move-explanations", post(move_explanations))
}

/// Count what each of two checker plays leaves on the board.
#[utoipa::path(post, path = "/api/move-explanations", request_body = MoveExplanationRequest,
    responses((status = 200, body = MoveExplanation), (status = 422, body = ErrorBody)),
    tag = "Positions")]
async fn move_explanations(
    payload: Result<Json<MoveExplanationRequest>, JsonRejection>,
) -> ApiResult<Json<MoveExplanation>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    explain(&request).map(Json).map_err(ApiError::invalid)
}

#[derive(OpenApi)]
#[openapi(
    paths(move_explanations),
    components(schemas(MoveExplanationRequest, MoveExplanation, PlayFacts))
)]
pub struct MoveExplanationDoc;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::sgf::initial_board;

    fn opening(player: i32, best: &str, played: &str) -> MoveExplanation {
        explain(&MoveExplanationRequest {
            board: initial_board(),
            player,
            dice: [6, 1],
            best: best.into(),
            played: played.into(),
        })
        .unwrap()
    }

    #[test]
    fn opening_six_one_counts_points_blots_and_shots() {
        let result = opening(0, "13/7 8/7", "8/2 6/5");
        // Making the bar point leaves no blot and a three-point block.
        assert_eq!(result.best.blots, Vec::<i32>::new());
        assert_eq!(result.best.hitting_rolls, 0);
        assert_eq!(result.best.points, vec![6, 7, 8, 13, 24]);
        assert_eq!(result.best.longest_prime, 3);
        // Blots on the 2 and 5 points face the two checkers on the 1 point:
        // any 1 (11 rolls), any 4 without a 1 (9 rolls) and 2-2 hit.
        assert_eq!(result.played.blots, vec![2, 5]);
        assert_eq!(result.played.hitting_rolls, 21);
        assert_eq!(result.played.points, vec![6, 8, 13, 24]);
        assert_eq!(result.played.longest_prime, 1);
        assert_eq!(result.best.pips, 160);
        assert_eq!(result.played.pips, 160);
        assert_eq!(result.best.opponent_pips, 167);
    }

    #[test]
    fn both_colors_see_the_same_facts_on_the_mirrored_board() {
        // Black's opening points mirror White's; Black's 13/7 8/7 is the same play.
        let white = opening(0, "13/7 8/7", "24/23 13/7");
        let black = opening(1, "13/7 8/7", "24/23 13/7");
        assert_eq!(white.best, black.best);
        assert_eq!(white.played, black.played);
        // Splitting the back checkers and slotting the bar point leaves three blots.
        assert_eq!(white.played.blots, vec![7, 23, 24]);
        assert_eq!(white.played.points, vec![6, 8, 13]);
    }

    #[test]
    fn hits_and_bar_entry_are_counted() {
        // White has a blot on the 5 point; Black has one checker on the bar.
        let mut board = [0; 26];
        board[5] = 1;
        board[6] = 2;
        board[0] = -1;
        let view = view(&board, 0);
        assert_eq!(view.opponent_bar, 1);
        // Entering with a 5 hits (11 rolls). 4-1 and 3-2 enter with one die and
        // hit with the other (4 rolls). 1-1 only reaches the 4 point, and no
        // other double lands on the 5 point.
        assert_eq!(hitting_rolls(&view), 15);
        let facts = facts(&board, 0);
        assert_eq!(facts.opponent_on_bar, 1);
        assert_eq!(facts.opponent_pips, 25);
    }

    #[test]
    fn invalid_requests_are_rejected() {
        let request = |dice, best: &str| MoveExplanationRequest {
            board: initial_board(),
            player: 0,
            dice,
            best: best.into(),
            played: "13/7 8/7".into(),
        };
        assert!(explain(&request([6, 1], "13/7 8/7")).is_ok());
        assert!(explain(&request([7, 1], "13/7 8/7")).is_err());
        assert!(explain(&request([6, 1], "24/1")).is_err());
        assert!(explain(&request([6, 1], "")).is_err());
    }
}
