use anyhow::{Result, ensure};

pub const MAX_CUBE_VALUE: i32 = 32768;

pub fn is_valid_cube_value(value: i32) -> bool {
    (1..=MAX_CUBE_VALUE).contains(&value) && (value as u32).is_power_of_two()
}

pub fn initial_board() -> [i32; 26] {
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

pub fn validate(board: &[i32; 26]) -> Result<()> {
    ensure!(
        board.iter().all(|count| (-15..=15).contains(count)),
        "Each board count must be between -15 and 15"
    );
    ensure!(board[0] <= 0 && board[25] >= 0, "invalid bar ownership");
    ensure!(
        board.iter().map(|x| (*x).max(0)).sum::<i32>() <= 15,
        "more than 15 white checkers"
    );
    ensure!(
        board.iter().map(|x| (-*x).max(0)).sum::<i32>() <= 15,
        "more than 15 black checkers"
    );
    Ok(())
}

pub fn validate_slice(board: &[i32]) -> Result<&[i32; 26]> {
    let array: &[i32; 26] = board
        .try_into()
        .map_err(|_| anyhow::anyhow!("Board must contain 26 points"))?;
    validate(array)?;
    Ok(array)
}

pub fn validate_active(board: &[i32; 26]) -> Result<()> {
    validate(board)?;
    ensure!(
        board.iter().any(|&count| count > 0) && board.iter().any(|&count| count < 0),
        "Position must have active checkers for both sides"
    );
    Ok(())
}

pub fn valid_board(b: &[i32; 26]) -> Result<()> {
    validate(b)
}

pub fn local_board(board: &[i32; 26], player: i32) -> [i32; 26] {
    if player == 0 {
        *board
    } else {
        std::array::from_fn(|i| -board[25 - i])
    }
}

/// Pip counts `[white_pips, black_pips]` for an absolute board, or
/// `[mover_pips, opponent_pips]` for a `local_board` (both use `board[25] >= 0`
/// and `board[0] <= 0`).
pub fn pip_counts(board: &[i32; 26]) -> [i32; 2] {
    let mut white = 25 * board[25].max(0);
    let mut black = 25 * (-board[0]).max(0);
    for (point, &count) in board.iter().enumerate().take(25).skip(1) {
        if count > 0 {
            white += point as i32 * count;
        } else if count < 0 {
            black += (25 - point) as i32 * (-count);
        }
    }
    [white, black]
}

/// True while at least one negative checker still sits behind (at a lower point
/// than, or on the bar behind) the rearmost positive checker. Works identically
/// on absolute boards and mover-relative `local_board`s.
pub fn has_contact(board: &[i32; 26]) -> bool {
    let rearmost_positive = if board[25] > 0 {
        25
    } else {
        (1..=24).rev().find(|&p| board[p] > 0).unwrap_or(0)
    };
    let rearmost_negative = if board[0] < 0 {
        0
    } else {
        (1..=24).find(|&p| board[p] < 0).unwrap_or(25)
    };
    rearmost_negative < rearmost_positive
}

pub(super) fn is_finished(board: &[i32; 26]) -> bool {
    !board.iter().any(|&count| count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_boards_pips_contact_and_cube_values() {
        let start = initial_board();
        assert!(validate(&start).is_ok());
        assert!(validate_active(&start).is_ok());
        assert_eq!(pip_counts(&start), [167, 167]);
        assert!(has_contact(&start));

        let mut pure_race = [0; 26];
        pure_race[6] = 2;
        pure_race[19] = -2;
        assert_eq!(pip_counts(&pure_race), [12, 12]);
        assert!(!has_contact(&pure_race));

        let mut bar_contact = [0; 26];
        bar_contact[25] = 1;
        bar_contact[24] = -1;
        assert_eq!(pip_counts(&bar_contact), [25, 1]);
        assert!(has_contact(&bar_contact));

        let mut terminal = [0; 26];
        terminal[24] = -1;
        assert!(validate(&terminal).is_ok());
        assert!(validate_active(&terminal).is_err());
        assert!(!has_contact(&terminal));

        let mut bad_bar = [0; 26];
        bad_bar[0] = 1;
        assert!(validate(&bad_bar).is_err());

        assert!(is_valid_cube_value(1));
        assert!(is_valid_cube_value(64));
        assert!(is_valid_cube_value(32768));
        assert!(!is_valid_cube_value(0));
        assert!(!is_valid_cube_value(3));
        assert!(!is_valid_cube_value(65536));
    }
}
