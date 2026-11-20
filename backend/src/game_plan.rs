// SPDX-License-Identifier: MPL-2.0
// Copyright (C) 2026 Mark Higgins
// Rust adaptation for Neondojo. See game_plan_source.txt for provenance.

//! Deterministic, board-only Open Sage game-plan labels for absolute colors.

use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use axum::{Json, Router, extract::rejection::JsonRejection, routing::post};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

/// The pinned Open Sage rules with Neondojo's opening label on top.
pub const CLASSIFIER: &str = "opensage_rules_v1_opening1";
pub const SOURCE_REVISION: &str = "139427d826dd67798226e7d23c003c90315306ba";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GamePlan {
    /// Neondojo label: both sides are still close to the starting position.
    Opening,
    PureRace,
    Racing,
    Attacking,
    Priming,
    Anchoring,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GamePlanRequest {
    /// Absolute-color board: points 1–24 use positive White/negative Black;
    /// Black's bar at 0 is nonpositive, White's bar at 25 is nonnegative.
    /// Each side has at most 15 checkers; absent checkers are borne off.
    #[schema(min_items = 26, max_items = 26)]
    pub board: [i32; 26],
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GamePlans {
    pub classifier: &'static str,
    /// Pinned upstream Git commit; changes when the reference rules change.
    pub source_revision: &'static str,
    /// At least one side has no checkers remaining, including on the bar.
    /// Labels retain the original rule classification for terminal boards.
    pub game_over: bool,
    pub white: GamePlan,
    pub black: GamePlan,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/game-plans", post(game_plans))
}

/// Classify both absolute colors without a database, engine or network request.
pub fn classify(board: &[i32; 26]) -> Result<GamePlans, String> {
    let mut plans = classify_reference(board)?;
    // The reference rules label the starting position as anchoring. The first
    // moves of a game are their own subject, so they get their own label.
    if is_opening(board) {
        plans.white = GamePlan::Opening;
        plans.black = GamePlan::Opening;
    }
    Ok(plans)
}

/// The pinned Open Sage rules alone, without Neondojo's opening label.
pub fn classify_reference(board: &[i32; 26]) -> Result<GamePlans, String> {
    if board.iter().any(|&count| !(-15..=15).contains(&count)) {
        return Err("Each board count must be between -15 and 15".into());
    }
    if board[0] > 0 || board[25] < 0 {
        return Err(
            "Black's bar at 0 must be nonpositive and White's bar at 25 nonnegative".into(),
        );
    }
    let white_count: i32 = board.iter().map(|&count| count.max(0)).sum();
    let black_count: i32 = board.iter().map(|&count| (-count).max(0)).sum();
    if white_count > 15 || black_count > 15 {
        return Err("Each side may have at most 15 checkers, including the bar".into());
    }

    // Open Sage's opponent bar is positive, unlike Neondojo's absolute board.
    let mut white_board = *board;
    white_board[0] = -white_board[0];
    let black_board = flip(&white_board);
    Ok(GamePlans {
        classifier: CLASSIFIER,
        source_revision: SOURCE_REVISION,
        game_over: white_count == 0 || black_count == 0,
        white: classify_game_plan(&white_board),
        black: classify_game_plan(&black_board),
    })
}

/// Checkers of one side that may have left their starting points while the
/// position still counts as the opening: about two rolls per player.
const OPENING_MAX_DISPLACED: i32 = 4;

/// Both sides have at most four checkers off their starting points.
fn is_opening(board: &[i32; 26]) -> bool {
    let start = crate::import::sgf::initial_board();
    let displaced = |sign: i32| -> i32 {
        (1..=24)
            .map(|point| ((start[point] * sign).max(0) - (board[point] * sign).max(0)).max(0))
            .sum()
    };
    displaced(1) <= OPENING_MAX_DISPLACED && displaced(-1) <= OPENING_MAX_DISPLACED
}

fn flip(board: &[i32; 26]) -> [i32; 26] {
    std::array::from_fn(|point| match point {
        0 => board[25],
        25 => board[0],
        _ => -board[25 - point],
    })
}

fn player_checkers(board: &[i32; 26], first: usize, last: usize) -> i32 {
    board[first..=last].iter().map(|&n| n.max(0)).sum()
}

fn opponent_checkers(board: &[i32; 26], first: usize, last: usize) -> i32 {
    board[first..=last].iter().map(|&n| (-n).max(0)).sum()
}

fn is_race(board: &[i32; 26]) -> bool {
    if board[0] > 0 || board[25] > 0 {
        return false;
    }
    let furthest_player = (1..=24).rev().find(|&i| board[i] > 0).unwrap_or(0);
    let nearest_opponent = (1..=24).find(|&i| board[i] < 0).unwrap_or(25);
    furthest_player <= nearest_opponent
}

fn pip_counts(board: &[i32; 26]) -> (i32, i32) {
    let mut player = 25 * board[25];
    let mut opponent = 25 * board[0];
    for (point, &count) in board.iter().enumerate().take(25).skip(1) {
        if count > 0 {
            player += point as i32 * count;
        } else if count < 0 {
            opponent -= (25 - point) as i32 * count;
        }
    }
    (player, opponent)
}

pub(crate) fn absolute_pip_counts(board: &[i32; 26]) -> ([i32; 2], [i32; 2]) {
    let mut relative = *board;
    relative[0] = -relative[0];
    let (white, black) = pip_counts(&relative);
    ([white, black], [25 * relative[25], 25 * relative[0]])
}

// The branch order and thresholds match the pinned Open Sage implementation.
// Input is in its player-relative convention, with BOTH bars nonnegative.
fn classify_game_plan(board: &[i32; 26]) -> GamePlan {
    if is_race(board) {
        return GamePlan::PureRace;
    }
    let (player_pips, opponent_pips) = pip_counts(board);
    let opponent_back = board[0] + opponent_checkers(board, 1, 6);
    let player_back = board[25] + player_checkers(board, 19, 24);
    let player_borne = 15 - board[25] - player_checkers(board, 1, 24);
    let opponent_borne = 15 - board[0] - opponent_checkers(board, 1, 24);
    let player_offside = board[25] + player_checkers(board, 13, 24);
    // The upstream opponent offside interval deliberately includes point 13.
    let opponent_offside = board[0] + opponent_checkers(board, 1, 13);

    if opponent_back == 0 {
        if player_pips < opponent_pips {
            return GamePlan::Racing;
        }
        if player_back > 1 {
            return GamePlan::Anchoring;
        }
        if opponent_borne > player_borne && opponent_offside == 0 {
            return GamePlan::Attacking;
        }
        return GamePlan::Racing;
    }

    if player_pips < opponent_pips {
        let outside_home = player_checkers(board, 7, 12);
        if player_offside == 0 && outside_home < 4 {
            return GamePlan::Racing;
        }
        let zone = player_checkers(board, 1, 12);
        if zone >= 8 {
            let opponent_anchors = board[1..=6].iter().filter(|&&n| n < -1).count();
            if opponent_anchors >= 2 {
                return GamePlan::Racing;
            }
            if board[0] > 0 {
                return GamePlan::Attacking;
            }
            if let Some(blot) = (1..=6).find(|&i| board[i] == -1) {
                let checker_above = (blot + 1..=6).any(|i| board[i] > 0);
                return if checker_above {
                    GamePlan::Attacking
                } else {
                    GamePlan::Racing
                };
            }
            if player_back < 2 {
                let mut opponent_max = if board[0] > 0 { Some(0) } else { None };
                for (point, &count) in board.iter().enumerate().take(6).skip(1) {
                    if count < 0 {
                        opponent_max = Some(point);
                    }
                }
                if let Some(opponent_max) = opponent_max {
                    let mut max_prime = 0;
                    let mut prime = 0;
                    for &count in &board[opponent_max + 1..=11] {
                        if count > 1 {
                            prime += 1;
                            max_prime = max_prime.max(prime);
                        } else {
                            prime = 0;
                        }
                    }
                    if max_prime >= 3 {
                        return GamePlan::Priming;
                    }
                }
                return GamePlan::Racing;
            }
            return GamePlan::Attacking;
        }
        return GamePlan::Racing;
    }

    // Upstream treats equal pip counts as the behind/equal branch.
    if player_borne > opponent_borne && player_offside == 0 {
        return GamePlan::Racing;
    }
    let home_anchors = board[1..=6].iter().filter(|&&n| n > 1).count();
    let deep_point = board[1..=3].iter().any(|&n| n >= 2);
    let gap = board[4..=6].iter().any(|&n| n <= 0);
    let pip_diff_ratio = if player_pips + opponent_pips > 0 {
        2.0 * f64::from(player_pips - opponent_pips) / f64::from(player_pips + opponent_pips)
    } else {
        0.0
    };
    if home_anchors < 4 && deep_point && gap {
        if pip_diff_ratio <= 0.1 {
            return GamePlan::Attacking;
        }
        return if player_back < 2 {
            GamePlan::Racing
        } else {
            GamePlan::Anchoring
        };
    }

    let opponent_high_anchor = board[4..=6].iter().any(|&n| n <= -2);
    let opponent_blots =
        i32::from(board[0] > 0) + board[1..=12].iter().filter(|&&n| n == -1).count() as i32;
    if opponent_high_anchor {
        if player_back >= 2 {
            return GamePlan::Anchoring;
        }
        if opponent_blots > 0 && pip_diff_ratio <= 0.1 {
            return GamePlan::Attacking;
        }
        return GamePlan::Racing;
    }

    let primers = board[4..=12].iter().filter(|&&n| n >= 1).count();
    let primers_bar = board[5..=8].iter().filter(|&&n| n >= 1).count();
    if primers >= 4 || primers_bar >= 3 {
        return GamePlan::Priming;
    }
    if opponent_blots > 0 && pip_diff_ratio <= 0.1 {
        return GamePlan::Attacking;
    }
    if player_back >= 2 {
        GamePlan::Anchoring
    } else {
        GamePlan::Priming
    }
}

#[utoipa::path(post, path="/api/game-plans", request_body=GamePlanRequest, responses((status=200,body=GamePlans),(status=422,body=ErrorBody)), tag="Positions")]
async fn game_plans(
    payload: Result<Json<GamePlanRequest>, JsonRejection>,
) -> ApiResult<Json<GamePlans>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    classify(&request.board)
        .map(Json)
        .map_err(ApiError::invalid)
}

#[derive(OpenApi)]
#[openapi(
    paths(game_plans),
    components(schemas(GamePlanRequest, GamePlan, GamePlans))
)]
pub struct GamePlanDoc;
