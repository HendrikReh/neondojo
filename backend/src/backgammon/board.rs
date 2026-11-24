use anyhow::{Result, ensure};

pub(crate) fn initial_board() -> [i32; 26] {
    let mut b = [0; 26];
    for (p, n) in [
        (6, 5),
        (8, 3),
        (13, 5),
        (24, 2),
        (1, -2),
        (12, -5),
        (17, -3),
        (19, -5),
    ] {
        b[p] = n;
    }
    b
}

pub(crate) fn valid_board(b: &[i32; 26]) -> Result<()> {
    ensure!(b[0] <= 0 && b[25] >= 0, "SGF: invalid bar ownership");
    ensure!(
        b.iter().map(|x| x.max(&0)).sum::<i32>() <= 15,
        "SGF: more than 15 white checkers"
    );
    ensure!(
        b.iter().map(|x| (-x).max(0)).sum::<i32>() <= 15,
        "SGF: more than 15 black checkers"
    );
    Ok(())
}

pub(crate) fn local_board(board: &[i32; 26], player: i32) -> [i32; 26] {
    if player == 0 {
        *board
    } else {
        std::array::from_fn(|i| -board[25 - i])
    }
}

pub(super) fn is_finished(board: &[i32; 26]) -> bool {
    !board.iter().any(|&count| count > 0)
}
