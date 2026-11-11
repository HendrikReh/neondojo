//! Results are a read-only explanation of source evidence, never a score authority.
use crate::model::{GameResult, GameResultReason};

pub(crate) fn derive(
    board: &[i32; 26],
    cube: i32,
    jacoby_suppressed: bool,
    scores: [i32; 2],
    terminal_checker: bool,
    dropped_by: Option<i32>,
    recorded: Option<&str>,
) -> Option<GameResult> {
    let observed = if let Some(loser) = dropped_by {
        Some((1 - loser, 1, GameResultReason::Drop))
    } else if terminal_checker {
        let white: i32 = board.iter().map(|n| (*n).max(0)).sum();
        let black: i32 = board.iter().map(|n| (-*n).max(0)).sum();
        let winner = match (white, black) {
            (0, n) if n > 0 => Some(0),
            (n, 0) if n > 0 => Some(1),
            _ => None,
        };
        winner.map(|winner| {
            let loser_count = if winner == 0 { black } else { white };
            let in_home_or_bar = if winner == 0 {
                board[0] < 0 || board[1..=6].iter().any(|n| *n < 0)
            } else {
                board[25] > 0 || board[19..=24].iter().any(|n| *n > 0)
            };
            let multiplier = if jacoby_suppressed || loser_count < 15 {
                1
            } else if in_home_or_bar {
                3
            } else {
                2
            };
            let reason = match multiplier {
                3 => GameResultReason::Backgammon,
                2 => GameResultReason::Gammon,
                _ => GameResultReason::Single,
            };
            (winner, multiplier, reason)
        })
    } else {
        None
    };
    let explicit = recorded.and_then(parse_recorded);
    let (winner, points, multiplier, reason) = match (explicit, observed) {
        (Some((winner, points, resignation)), observed) => {
            if !resignation
                && let Some((side, multiplier, reason)) = observed
                && side == winner
                && cube.checked_mul(multiplier) == Some(points)
            {
                (winner, points, Some(multiplier), reason)
            } else if resignation && observed.is_none() {
                let multiplier = (points % cube == 0)
                    .then_some(points / cube)
                    .filter(|n| (1..=3).contains(n))
                    .filter(|n| !jacoby_suppressed || *n == 1);
                (winner, points, multiplier, GameResultReason::Resignation)
            } else {
                (winner, points, None, GameResultReason::Recorded)
            }
        }
        (None, Some((winner, multiplier, reason))) if recorded.is_none() => (
            winner,
            cube.checked_mul(multiplier)?,
            Some(multiplier),
            reason,
        ),
        _ => return None,
    };
    let mut scores_after = scores;
    scores_after[winner as usize] = scores_after[winner as usize].checked_add(points)?;
    Some(GameResult {
        winner,
        points,
        cube,
        multiplier,
        reason,
        scores_after,
    })
}

fn parse_recorded(value: &str) -> Option<(i32, i32, bool)> {
    let (color, amount) = value.split_once('+')?;
    let winner = match color {
        "W" => 0,
        "B" => 1,
        _ => return None,
    };
    let resignation = amount.ends_with(['R', 'r']);
    let amount = if resignation {
        &amount[..amount.len() - 1]
    } else {
        amount
    };
    if amount.is_empty() || !amount.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let points: i32 = amount.parse().ok()?;
    (points > 0 && points <= 32767).then_some((winner, points, resignation))
}

/// Explicit next-game scores can establish an award even when the producer
/// omitted the terminal action. They cannot establish how that award was won.
pub(crate) fn from_following_scores(
    scores: [i32; 2],
    scores_after: [i32; 2],
    cube: i32,
) -> Option<GameResult> {
    let deltas = [
        scores_after[0].checked_sub(scores[0])?,
        scores_after[1].checked_sub(scores[1])?,
    ];
    let (winner, points) = match deltas {
        [points, 0] if points > 0 => (0, points),
        [0, points] if points > 0 => (1, points),
        _ => return None,
    };
    Some(GameResult {
        winner,
        points,
        cube,
        multiplier: None,
        reason: GameResultReason::Recorded,
        scores_after,
    })
}
