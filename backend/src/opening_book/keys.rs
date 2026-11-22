use super::*;

pub(crate) fn canonical_keys(position: &Position) -> Result<(String, Option<String>)> {
    use crate::import::ids::{Field, MatchId};
    ensure!(matches!(position.player, 0 | 1), "Invalid player");
    ensure!(
        position.board.len() == 26 && position.board[0] <= 0 && position.board[25] >= 0,
        "Invalid board orientation"
    );
    ensure!(
        position.board.iter().all(|n| (-15..=15).contains(n)),
        "Invalid checker counts"
    );
    for side in [1, -1] {
        let count: i32 = position.board.iter().map(|n| (n * side).max(0)).sum();
        ensure!(
            (1..=15).contains(&count),
            "Unsupported board variant or finished game"
        );
    }
    let id =
        MatchId::decode(&position.gnubg_match_id).context("GNUbg Match ID must encode 72 bits")?;
    let bits = |field| id.get(field);
    ensure!(
        bits(Field::EXTENSION) == 0,
        "Unsupported GNUbg Match ID extension"
    );
    ensure!(
        bits(Field::GAME_STATE) == 1 && bits(Field::DOUBLED) == 0 && bits(Field::RESIGNED) == 0,
        "Game is not an ordinary checker decision"
    );
    ensure!(
        bits(Field::ON_ROLL) == position.player && bits(Field::TURN) == position.player,
        "Mover does not match GNUbg Match ID"
    );
    let mut dice = [bits(Field::DIE_0), bits(Field::DIE_1)];
    ensure!(
        dice.iter().all(|die| (1..=6).contains(die)),
        "Checker dice are not rolled"
    );
    let mut stored_dice = position.dice.clone();
    stored_dice.sort_unstable();
    dice.sort_unstable();
    ensure!(stored_dice == dice, "Dice do not match GNUbg Match ID");
    let roll = format!("{}{}", dice[1], dice[0]);
    let mut length = bits(Field::LENGTH);
    let mut scores = [bits(Field::SCORE_0), bits(Field::SCORE_1)];
    let crawford = bits(Field::CRAWFORD) == 1;
    ensure!(
        !crawford
            || (length > 0 && scores.iter().filter(|score| **score == length - 1).count() == 1),
        "Invalid Crawford state"
    );
    if length > 0 {
        ensure!(
            scores.iter().all(|score| *score < length),
            "Match has already ended"
        );
        let away = [length - scores[0], length - scores[1]];
        ensure!(
            away.iter().all(|n| *n <= 25),
            "Away score exceeds released book coverage"
        );
        length = away[0].max(away[1]);
        scores = [length - away[0], length - away[1]];
    } else {
        scores = [0, 0];
    }
    let flags = if length > 0 {
        i32::from(crawford)
    } else {
        1 - bits(Field::NO_JACOBY)
    };
    let owner = bits(Field::CUBE_OWNER);
    ensure!(matches!(owner, 0 | 1 | 3), "Invalid cube owner");
    let mut cube_exp = bits(Field::CUBE_LOG2);
    ensure!(
        !crawford || (cube_exp == 0 && owner == 3),
        "Cube is turned in Crawford game"
    );
    ensure!(cube_exp > 0 || owner == 3, "Owned cube at value one");
    if length == 0 && owner == 3 {
        cube_exp = 0;
    }
    // Check the redundant Position ID too; never silently normalize inconsistent imports.
    // The checks above bound each side to 15 checkers, so the board fits 80 bits.
    let board: &[i32; 26] = position.board.as_slice().try_into()?;
    ensure!(
        crate::import::ids::position_id(board, position.player) == position.position_id,
        "Board does not match GNUbg Position ID"
    );
    let mover = position.player as usize;
    let board = if mover == 0 {
        position.board.clone()
    } else {
        position.board.iter().rev().map(|n| -n).collect()
    };
    let board: String = board
        .iter()
        .map(|n| match *n {
            0 => '-',
            n if n > 0 => (b'A' + (n - 1) as u8) as char,
            n => (b'a' + (-n - 1) as u8) as char,
        })
        .collect();
    let xg_owner = if owner == 3 {
        0
    } else if owner == position.player {
        1
    } else {
        -1
    };
    let key = format!(
        "{board}:{cube_exp}:{xg_owner}:1:{roll}:{}:{}:{flags}:{length}:10",
        scores[mover],
        scores[1 - mover]
    );
    let cube = (!crawford && (owner == 3 || owner == position.player))
        .then(|| key.replacen(&format!(":{roll}:"), ":00:", 1));
    Ok((key, cube))
}
