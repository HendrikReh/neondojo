//! GNUbg Position ID and Match ID codecs. Every module reads and writes these
//! identifiers through this file.

use anyhow::{Context as _, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};

/// A field of GNUbg's 72-bit Match ID: `(offset, width)` in bits, least
/// significant bit first.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Field(usize, usize);

impl Field {
    /// log2 of the cube value.
    pub(crate) const CUBE_LOG2: Self = Self(0, 4);
    /// 0 or 1 for the owner, 3 for a centered cube.
    pub(crate) const CUBE_OWNER: Self = Self(4, 2);
    /// The player on roll.
    pub(crate) const ON_ROLL: Self = Self(6, 1);
    pub(crate) const CRAWFORD: Self = Self(7, 1);
    /// 1 while a game is being played.
    pub(crate) const GAME_STATE: Self = Self(8, 3);
    /// The player who must make a decision.
    pub(crate) const TURN: Self = Self(11, 1);
    pub(crate) const DOUBLED: Self = Self(12, 1);
    pub(crate) const RESIGNED: Self = Self(13, 2);
    pub(crate) const DIE_0: Self = Self(15, 3);
    pub(crate) const DIE_1: Self = Self(18, 3);
    /// 0 for money play.
    pub(crate) const LENGTH: Self = Self(21, 15);
    pub(crate) const SCORE_0: Self = Self(36, 15);
    pub(crate) const SCORE_1: Self = Self(51, 15);
    /// Set when the Jacoby rule is off; GNUbg also sets it in match play.
    pub(crate) const NO_JACOBY: Self = Self(66, 1);
    /// Reserved bits after the GNUbg fields.
    pub(crate) const EXTENSION: Self = Self(67, 5);
}

pub(crate) struct MatchId([u8; 9]);

impl MatchId {
    pub(crate) fn decode(id: &str) -> Option<Self> {
        Some(Self(STANDARD_NO_PAD.decode(id).ok()?.try_into().ok()?))
    }

    pub(crate) fn encode(&self) -> String {
        STANDARD_NO_PAD.encode(self.0)
    }

    pub(crate) fn get(&self, Field(offset, width): Field) -> i32 {
        (0..width).fold(0, |value, i| {
            value | (i32::from((self.0[(offset + i) / 8] >> ((offset + i) % 8)) & 1) << i)
        })
    }

    pub(crate) fn set(&mut self, Field(offset, width): Field, value: i32) {
        for i in 0..width {
            let (byte, bit) = ((offset + i) / 8, (offset + i) % 8);
            self.0[byte] &= !(1 << bit);
            if (value >> i) & 1 != 0 {
                self.0[byte] |= 1 << bit;
            }
        }
    }
}

/// The player on roll in a Match ID.
pub(crate) fn on_roll(id: &str) -> Option<i32> {
    Some(MatchId::decode(id)?.get(Field::ON_ROLL))
}

/// The same Match ID before the dice are rolled.
pub(crate) fn hide_dice(id: &str) -> Option<String> {
    let mut id = MatchId::decode(id)?;
    id.set(Field::DIE_0, 0);
    id.set(Field::DIE_1, 0);
    Some(id.encode())
}

/// The native mistake browser displays a responder-oriented ID with a visual
/// offer bit. For evaluation, restore the responder-oriented decision ID.
pub(crate) fn without_offer_marker(id: &str) -> Option<String> {
    let mut id = MatchId::decode(id)?;
    id.set(Field::DOUBLED, 0);
    Some(id.encode())
}

/// Marks a responder-oriented decision ID as facing an offered double.
pub(crate) fn with_offer_marker(id: &str) -> Option<String> {
    let mut id = MatchId::decode(id)?;
    id.set(Field::DOUBLED, 1);
    Some(id.encode())
}

/// GNUbg encodes the opponent's checkers first, then the player on roll.
pub(crate) fn position_id(board: &[i32; 26], player: i32) -> String {
    let mut bytes = [0u8; 10];
    let mut bit = 0usize;
    for side in [1 - player, player] {
        for point in 1..=25 {
            let index = if side == 0 { point } else { 25 - point };
            let n = if side == 0 {
                board[index].max(0)
            } else {
                (-board[index]).max(0)
            };
            for _ in 0..n {
                bytes[bit / 8] |= 1 << (bit % 8);
                bit += 1;
            }
            bit += 1;
        }
    }
    STANDARD_NO_PAD.encode(bytes)
}

/// Inverse of [`position_id`] for the player on roll in `match_id`, returned
/// in absolute colors.
pub(crate) fn decode_board(position_id: &str, match_id: &str) -> anyhow::Result<Vec<i32>> {
    let key = STANDARD_NO_PAD.decode(position_id)?;
    ensure!(key.len() == 10, "Invalid GNU ID length");
    let player = on_roll(match_id).context("Invalid GNU ID length")?;
    let mut board = vec![0; 26];
    let mut bit = 0usize;
    for side in [1 - player, player] {
        let mut total = 0;
        for point in 1..=25 {
            let mut count = 0;
            while bit < 80 && (key[bit / 8] >> (bit % 8)) & 1 == 1 {
                count += 1;
                bit += 1;
            }
            ensure!(bit < 80, "Invalid unary GNU position ID");
            bit += 1;
            total += count;
            ensure!(total <= 15, "Invalid checker count in GNU position ID");
            let index = if side == 0 { point } else { 25 - point };
            if count > 0 {
                ensure!(board[index] == 0, "Opposing checkers share a point");
                board[index] = if side == 0 { count } else { -count };
            }
        }
    }
    ensure!(
        (bit..80).all(|i| key[i / 8] >> (i % 8) & 1 == 0),
        "Noncanonical GNU position ID"
    );
    Ok(board)
}

// These are the eight independently encoded fields in GNUbg's binary Match ID.
#[allow(clippy::too_many_arguments)]
pub(crate) fn match_id(
    length: i32,
    scores: [i32; 2],
    cube: i32,
    owner: i32,
    crawford: bool,
    jacoby: bool,
    player: i32,
    dice: [i32; 2],
) -> String {
    let mut id = MatchId([0; 9]);
    id.set(Field::CUBE_LOG2, cube.trailing_zeros() as i32);
    id.set(Field::CUBE_OWNER, owner & 3);
    id.set(Field::ON_ROLL, player);
    id.set(Field::CRAWFORD, i32::from(crawford));
    id.set(Field::GAME_STATE, 1);
    id.set(Field::TURN, player);
    id.set(Field::DIE_0, dice[0].max(dice[1]));
    id.set(Field::DIE_1, dice[0].min(dice[1]));
    id.set(Field::LENGTH, length);
    id.set(Field::SCORE_0, scores[0]);
    id.set(Field::SCORE_1, scores[1]);
    id.set(Field::NO_JACOBY, i32::from(!jacoby));
    id.encode()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference IDs printed by GNU Backgammon 1.08.003: the opening position,
    // and a 7-point match at 2-4 with player 1 on roll, owning a 2-cube,
    // after rolling 5-2.
    const OPENING: &str = "4HPwATDgc/ABMA";
    const MATCH_2_4: &str = "UYnqACAAIAAE";

    #[test]
    fn ids_equal_gnubg_reference_ids() {
        let board = crate::import::sgf::initial_board();
        for player in [0, 1] {
            assert_eq!(position_id(&board, player), OPENING);
            let on_roll = match_id(0, [0, 0], 1, 3, false, true, player, [0, 0]);
            assert_eq!(decode_board(OPENING, &on_roll).unwrap(), board.to_vec());
        }
        assert_eq!(
            match_id(7, [2, 4], 2, 1, false, false, 1, [5, 2]),
            MATCH_2_4
        );
    }

    #[test]
    fn fields_read_the_gnubg_reference_match_id() {
        let id = MatchId::decode(MATCH_2_4).unwrap();
        for (field, value) in [
            (Field::CUBE_LOG2, 1),
            (Field::CUBE_OWNER, 1),
            (Field::ON_ROLL, 1),
            (Field::CRAWFORD, 0),
            (Field::GAME_STATE, 1),
            (Field::TURN, 1),
            (Field::DOUBLED, 0),
            (Field::RESIGNED, 0),
            (Field::DIE_0, 5),
            (Field::DIE_1, 2),
            (Field::LENGTH, 7),
            (Field::SCORE_0, 2),
            (Field::SCORE_1, 4),
            (Field::NO_JACOBY, 1),
            (Field::EXTENSION, 0),
        ] {
            assert_eq!(id.get(field), value, "{field:?}");
        }
        assert_eq!(on_roll(MATCH_2_4), Some(1));
    }

    #[test]
    fn set_replaces_a_field_without_touching_its_neighbours() {
        let mut id = MatchId::decode(MATCH_2_4).unwrap();
        id.set(Field::DIE_0, 0);
        id.set(Field::DIE_1, 0);
        assert_eq!(
            id.encode(),
            match_id(7, [2, 4], 2, 1, false, false, 1, [0, 0])
        );
        assert_eq!(hide_dice(MATCH_2_4).unwrap(), id.encode());
        let offered = with_offer_marker(&id.encode()).unwrap();
        assert_eq!(MatchId::decode(&offered).unwrap().get(Field::DOUBLED), 1);
        assert_eq!(without_offer_marker(&offered).unwrap(), id.encode());
    }

    #[test]
    fn invalid_ids_are_rejected() {
        assert!(MatchId::decode("AAAA").is_none());
        assert!(decode_board("AAAAAAAAAAAAAA", "AAAAAAAAAAAA").is_ok());
        assert!(decode_board("/////////////w", "AAAAAAAAAAAA").is_err());
        assert!(decode_board(OPENING, "AAAA").is_err());
    }
}
