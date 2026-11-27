use super::*;
use crate::backgammon::board::{local_board, validate_active, validate_slice};
use crate::backgammon::ids::{Field, MatchId};

/// Validated, mover-relative opening-book lookup key shared across the
/// BackgammonDB SQLite book and the XG binary opening book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CanonicalKey {
    pub(super) board: [i32; 26],
    /// Actual `log2(cube)` from the GNUbg Match ID (`0..=15`).
    /// BackgammonDB collapses centered money cubes to `0` via `bgdb_cube_exp()`,
    /// while the XG book keeps the actual cube value.
    pub(super) cube_exp: i32,
    /// Mover-relative XG cube owner (`0` centered, `1` mover, `-1` opponent).
    pub(super) xg_owner: i32,
    /// Sorted descending (`[high, low]`).
    pub(super) dice: [i32; 2],
    /// Mover-relative normalized scores (`[mover_score, opponent_score]`).
    pub(super) scores: [i32; 2],
    /// Crawford flag (`0`/`1`) in match play, or Jacoby flag (`0`/`1`) in money play.
    pub(super) flags: i32,
    /// Normalized match length (`max(away)`), or `0` for money play.
    pub(super) length: i32,
    pub(super) can_double: bool,
}

impl CanonicalKey {
    pub(super) fn from_position(position: &Position) -> Result<Self> {
        ensure!(matches!(position.player, 0 | 1), "Invalid player");
        let abs_board = validate_slice(&position.board)?;
        validate_active(abs_board)?;

        let id = MatchId::decode(&position.gnubg_match_id)
            .context("GNUbg Match ID must encode 72 bits")?;
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
        let dice = [dice[1], dice[0]];

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
        let cube_exp = bits(Field::CUBE_LOG2);
        ensure!(
            !crawford || (cube_exp == 0 && owner == 3),
            "Cube is turned in Crawford game"
        );
        ensure!(cube_exp > 0 || owner == 3, "Owned cube at value one");
        // Check the redundant Position ID too; never silently normalize inconsistent imports.
        // The checks above bound each side to 15 checkers, so the board fits 80 bits.
        ensure!(
            crate::backgammon::ids::position_id(abs_board, position.player) == position.position_id,
            "Board does not match GNUbg Position ID"
        );

        let mover = position.player as usize;
        let board = local_board(abs_board, position.player);
        let xg_owner = if owner == 3 {
            0
        } else if owner == position.player {
            1
        } else {
            -1
        };
        Ok(Self {
            board,
            cube_exp,
            xg_owner,
            dice,
            scores: [scores[mover], scores[1 - mover]],
            flags,
            length,
            can_double: id.can_double(position.player),
        })
    }

    pub(super) fn is_money(&self) -> bool {
        self.length == 0
    }

    pub(super) fn is_jacoby(&self) -> bool {
        self.length == 0 && self.flags == 1
    }

    pub(super) fn is_crawford(&self) -> bool {
        self.length > 0 && self.flags == 1
    }

    pub(super) fn away(&self) -> [i32; 2] {
        [self.length - self.scores[0], self.length - self.scores[1]]
    }

    pub(super) fn board_i8(&self) -> [i8; 26] {
        self.board.map(|n| n as i8)
    }

    fn board_string(&self) -> String {
        self.board
            .iter()
            .map(|n| match *n {
                0 => '-',
                n if n > 0 => (b'A' + (n - 1) as u8) as char,
                n => (b'a' + (-n - 1) as u8) as char,
            })
            .collect()
    }

    fn bgdb_cube_exp(&self) -> i32 {
        if self.length == 0 && self.xg_owner == 0 {
            0
        } else {
            self.cube_exp
        }
    }

    fn format_bgdb_xgid(&self, roll: &str) -> String {
        format!(
            "{}:{}:{}:1:{roll}:{}:{}:{}:{}:10",
            self.board_string(),
            self.bgdb_cube_exp(),
            self.xg_owner,
            self.scores[0],
            self.scores[1],
            self.flags,
            self.length
        )
    }

    pub(super) fn checker_xgid(&self) -> String {
        self.format_bgdb_xgid(&format!("{}{}", self.dice[0], self.dice[1]))
    }

    pub(super) fn cube_xgid(&self) -> Option<String> {
        self.can_double.then(|| self.format_bgdb_xgid("00"))
    }

    /// Position-level XGID preserving the actual cube and money beaver flag.
    pub(super) fn xg_position_key(&self, beaver: bool) -> String {
        let flags = if self.is_money() {
            i32::from(self.is_jacoby()) + 2 * i32::from(beaver)
        } else {
            self.flags
        };
        format!(
            "{}:{}:{}:1:00:{}:{}:{}:{}:10",
            self.board_string(),
            self.cube_exp,
            self.xg_owner,
            self.scores[0],
            self.scores[1],
            flags,
            self.length
        )
    }
}

#[cfg(test)]
pub(super) fn canonical_keys(position: &Position) -> Result<(String, Option<String>)> {
    let key = CanonicalKey::from_position(position)?;
    Ok((key.checker_xgid(), key.cube_xgid()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backgammon::{ids, initial_board};

    fn position(match_id: String, player: i32, dice: [i32; 2]) -> Position {
        let board = initial_board();
        Position {
            cube_enabled: None,
            id: uuid::Uuid::nil(),
            match_uuid: uuid::Uuid::nil(),
            ordinal: 0,
            game_no: 1,
            ply: 1,
            player,
            dice: dice.to_vec(),
            board: board.to_vec(),
            position_id: ids::position_id(&board, player),
            gnubg_match_id: match_id,
            notation: String::new(),
        }
    }

    #[test]
    fn canonical_keys_omits_cube_lookup_when_cube_is_dead() {
        // 7-point post-Crawford at 6-2: White (1-away) has a dead 1-cube, Black (5-away) can double.
        let white_dead = position(
            ids::match_id(7, [6, 2], 1, -1, false, false, 0, [3, 1]),
            0,
            [3, 1],
        );
        let (checker, cube) = canonical_keys(&white_dead).unwrap();
        assert!(!checker.is_empty());
        assert!(cube.is_none());

        let black_live = position(
            ids::match_id(7, [6, 2], 1, -1, false, false, 1, [3, 1]),
            1,
            [3, 1],
        );
        assert!(canonical_keys(&black_live).unwrap().1.is_some());

        // 7-point match at 5-2 with White owning a 2-cube: dead for White (5 + 2 == 7).
        let owned_dead = position(
            ids::match_id(7, [5, 2], 2, 0, false, false, 0, [4, 2]),
            0,
            [4, 2],
        );
        assert!(canonical_keys(&owned_dead).unwrap().1.is_none());

        // 7-point match at 4-2 with White owning a 2-cube: live for White (4 + 2 < 7).
        let owned_live = position(
            ids::match_id(7, [4, 2], 2, 0, false, false, 0, [4, 2]),
            0,
            [4, 2],
        );
        assert!(canonical_keys(&owned_live).unwrap().1.is_some());
    }
}
