use super::board::{is_finished, local_board, valid_board};
use anyhow::{Context, Result, ensure};

fn step(b: &[i32; 26], from: usize, to: usize, die: i32) -> Option<[i32; 26]> {
    if !(1..=25).contains(&from) || to >= from || b[from] <= 0 || (b[25] > 0 && from != 25) {
        return None;
    }
    if to > 0 {
        if from - to != die as usize || b[to] < -1 {
            return None;
        }
    } else {
        if from > 6 || b[7..=25].iter().any(|n| *n > 0) {
            return None;
        }
        if die < from as i32 || (die > from as i32 && b[from + 1..=6].iter().any(|n| *n > 0)) {
            return None;
        }
    }
    let mut out = *b;
    out[from] -= 1;
    if to > 0 {
        if out[to] == -1 {
            out[to] = 0;
            out[0] -= 1;
        }
        out[to] += 1;
    }
    Some(out)
}

fn any_step(board: &[i32; 26], die: i32) -> bool {
    (1..=25).any(|from| step(board, from, from.saturating_sub(die as usize), die).is_some())
}

fn turn_dice(dice: [i32; 2]) -> Vec<i32> {
    if dice[0] == dice[1] {
        vec![dice[0]; 4]
    } else {
        dice.to_vec()
    }
}

pub(crate) struct CheckerPath {
    pub board: [i32; 26],
    pub steps: Vec<(usize, usize, i32)>,
}

/// Complete checker paths in mover-relative coordinates, with absolute end
/// boards. An active position with no legal path is a forced pass.
pub(crate) fn checker_paths(
    board: &[i32; 26],
    player: i32,
    dice: [i32; 2],
) -> Result<Vec<CheckerPath>> {
    ensure!(matches!(player, 0 | 1), "Invalid checker turn player");
    ensure!(
        dice.iter().all(|die| (1..=6).contains(die)),
        "Invalid checker turn dice"
    );
    ensure!(
        board.iter().all(|count| (-15..=15).contains(count)),
        "Invalid checker counts"
    );
    valid_board(board)?;
    ensure!(
        board.iter().any(|&count| count > 0) && board.iter().any(|&count| count < 0),
        "Checker turns require an active position"
    );

    let before = local_board(board, player);
    let available = turn_dice(dice);
    let required = max_plays(&before, &available);
    let high = dice[0].max(dice[1]);
    let higher_die = (dice[0] != dice[1] && any_step(&before, high)).then_some(high);
    let mut paths = Vec::new();
    if required > 0 {
        collect_checker_paths(&before, &available, &mut Vec::new(), required, &mut paths);
    }
    paths.retain_mut(|path| {
        if let ([(from, to, die)], Some(high)) = (path.steps.as_slice(), higher_die)
            && *die != high
            && !(is_finished(&path.board) && step(&before, *from, *to, high).is_some())
        {
            return false;
        }
        path.board = local_board(&path.board, player);
        true
    });
    Ok(paths)
}

fn collect_checker_paths(
    board: &[i32; 26],
    dice: &[i32],
    steps: &mut Vec<(usize, usize, i32)>,
    required: usize,
    paths: &mut Vec<CheckerPath>,
) {
    if steps.len() == required || is_finished(board) {
        paths.push(CheckerPath {
            board: *board,
            steps: steps.clone(),
        });
        return;
    }
    for (index, &die) in dice.iter().enumerate() {
        if index > 0 && dice[index - 1] == die {
            continue;
        }
        let mut remaining = dice.to_vec();
        remaining.remove(index);
        for from in 1usize..=25 {
            let to = from.saturating_sub(die as usize);
            if let Some(after) = step(board, from, to, die) {
                steps.push((from, to, die));
                collect_checker_paths(&after, &remaining, steps, required, paths);
                steps.pop();
            }
        }
    }
}

fn max_plays(board: &[i32; 26], dice: &[i32]) -> usize {
    let mut best = 0;
    for (i, die) in dice.iter().enumerate() {
        if i > 0 && dice[i - 1] == *die {
            continue;
        }
        let mut rest = dice.to_vec();
        rest.remove(i);
        for from in 1usize..=25 {
            if let Some(after) = step(board, from, from.saturating_sub(*die as usize), *die) {
                best = best.max(1 + max_plays(&after, &rest));
                if best == dice.len() {
                    return best;
                }
            }
        }
    }
    best
}

fn apply_pairs(board: &[i32; 26], moves: &[(usize, usize)], dice: &[i32]) -> Option<[i32; 26]> {
    let Some(&(from, to)) = moves.first() else {
        // Refuse missing checker moves when a remaining die is still playable.
        return (!dice.iter().any(|d| any_step(board, *d))).then_some(*board);
    };
    for (i, die) in dice.iter().enumerate() {
        if i > 0 && dice[i - 1] == *die {
            continue;
        }
        if let Some(next) = step(board, from, to, *die) {
            let mut rest = dice.to_vec();
            rest.remove(i);
            if let Some(result) = apply_pairs(&next, &moves[1..], &rest) {
                return Some(result);
            }
        }
    }
    None
}

type ExpandedPlay = ([i32; 26], Vec<(usize, usize)>);

fn expand_runs(
    board: &[i32; 26],
    runs: &[(usize, usize)],
    dice: &[i32],
    remaining: usize,
) -> Option<ExpandedPlay> {
    if runs.is_empty() {
        // Bearing off the final checker ends the game immediately, even when
        // playing a smaller die first could have consumed another die.
        return (remaining == 0 || is_finished(board)).then(|| (*board, Vec::new()));
    }
    if remaining == 0 || runs.len() > remaining || remaining > dice.len() {
        return None;
    }
    for (run, &(from, target)) in runs.iter().enumerate() {
        if runs[..run].contains(&(from, target)) {
            continue;
        }
        for (die_index, &die) in dice.iter().enumerate() {
            if die_index > 0 && dice[die_index - 1] == die {
                continue;
            }
            let to = from.saturating_sub(die as usize);
            // Joined notation has no intermediate hits. A hit must be an explicit
            // endpoint in the source, otherwise choosing a path would invent one.
            if to < target || (to != target && board[to] == -1) {
                continue;
            }
            let Some(next) = step(board, from, to, die) else {
                continue;
            };
            let mut next_runs = runs.to_vec();
            if to == target {
                next_runs.remove(run);
            } else {
                next_runs[run].0 = to;
            }
            let mut next_dice = dice.to_vec();
            next_dice.remove(die_index);
            if let Some((after, mut steps)) =
                expand_runs(&next, &next_runs, &next_dice, remaining - 1)
            {
                steps.insert(0, (from, to));
                return Some((after, steps));
            }
        }
    }
    None
}

/// Expand a play whose runs may join several dice into one From/To pair, as XG
/// files imported from text do. Expand at most four dice using the replayed
/// board, including bar priority and legal overshoots. SGF imports themselves
/// continue to require explicit steps.
pub(crate) fn expand_joined_play(
    board: &mut [i32; 26],
    player: i32,
    dice: [i32; 2],
    runs: &[(usize, usize)],
) -> Result<Vec<(usize, usize)>> {
    ensure!(dice.iter().all(|d| (1..=6).contains(d)), "XG: invalid dice");
    ensure!(
        runs.len() <= 4
            && runs
                .iter()
                .all(|&(from, to)| (1..=25).contains(&from) && to < from),
        "XG: invalid checker run"
    );
    let available = turn_dice(dice);
    let before = local_board(board, player);
    let required = max_plays(&before, &available);
    let (after, steps) = expand_runs(&before, runs, &available, required)
        .context("XG: checker runs cannot be expanded to a legal, complete play")?;
    if steps.len() == 1 && dice[0] != dice[1] {
        let high = dice[0].max(dice[1]);
        let (from, to) = steps[0];
        ensure!(
            step(&before, from, to, high).is_some() || !any_step(&before, high),
            "XG: single checker move must use the larger die"
        );
    }
    *board = local_board(&after, player);
    Ok(steps)
}

/// Apply a complete turn in the given order. Pairs are mover-relative, with
/// bar at 25 and off at 0. The input and output boards use absolute colors.
pub(crate) fn apply_checker_turn(
    board: &[i32; 26],
    player: i32,
    dice: [i32; 2],
    pairs: &[(usize, usize)],
) -> Result<[i32; 26]> {
    ensure!(
        matches!(player, 0 | 1) && dice.iter().all(|die| (1..=6).contains(die)),
        "Invalid checker turn context"
    );
    let available = turn_dice(dice);
    let before = local_board(board, player);
    let after =
        apply_pairs(&before, pairs, &available).context("Illegal or incomplete checker move")?;
    ensure!(
        pairs.len() == max_plays(&before, &available) || is_finished(&after),
        "SGF: checker play does not use the maximum possible dice"
    );
    if pairs.len() == 1 && dice[0] != dice[1] {
        let high = dice[0].max(dice[1]);
        let (from, to) = pairs[0];
        ensure!(
            step(&before, from, to, high).is_some() || !any_step(&before, high),
            "SGF: single checker move must use the larger die"
        );
    }
    Ok(local_board(&after, player))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joined_runs_interleave_bar_entries_before_continuing() {
        let mut board = [0; 26];
        board[25] = 2;
        board[1] = -1;
        let steps = expand_joined_play(&mut board, 0, [3, 3], &[(25, 19), (25, 19)]).unwrap();
        assert_eq!(&steps[..2], &[(25, 22), (25, 22)]);
        assert_eq!(steps.len(), 4);
        assert_eq!(board[25], 0);
        assert_eq!(board[19], 2);
    }

    #[test]
    fn joined_runs_do_not_invent_intermediate_hits() {
        let mut board = [0; 26];
        board[13] = 1;
        board[8] = -1;
        let steps = expand_joined_play(&mut board, 0, [5, 1], &[(13, 7)]).unwrap();
        assert_eq!(steps, vec![(13, 12), (12, 7)]);
        assert_eq!(board[8], -1);
        assert_eq!(board[0], 0);
        assert_eq!(board[7], 1);
        board[13] = 1;
        board[7] = 0;
        board[12] = -2;
        let before = board;
        assert!(expand_joined_play(&mut board, 0, [5, 1], &[(13, 7)]).is_err());
        assert_eq!(board, before);
    }

    #[test]
    fn joined_runs_keep_endpoint_hits_for_both_players() {
        for player in [0, 1] {
            let mut local = [0; 26];
            local[13] = 1;
            local[7] = -1;
            let mut board = if player == 0 {
                local
            } else {
                std::array::from_fn(|i| -local[25 - i])
            };
            let steps = expand_joined_play(&mut board, player, [5, 1], &[(13, 7)]).unwrap();
            assert_eq!(steps.len(), 2);
            let after = if player == 0 {
                board
            } else {
                std::array::from_fn(|i| -board[25 - i])
            };
            assert_eq!(after[13], 0);
            assert_eq!(after[7], 1);
            assert_eq!(after[0], -1);
        }
    }

    #[test]
    fn joined_bearing_off_and_single_die_priority_follow_the_rules() {
        let mut board = [0; 26];
        board[6] = 1;
        board[24] = -1;
        let steps = expand_joined_play(&mut board, 0, [5, 4], &[(6, 0)]).unwrap();
        assert_eq!(steps.len(), 2);
        assert!(!board.iter().any(|n| *n > 0));
        let mut board = [0; 26];
        board[25] = 1;
        board[14] = -2;
        let before = board;
        assert!(expand_joined_play(&mut board, 0, [6, 5], &[(25, 20)]).is_err());
        assert_eq!(board, before);
    }
}
