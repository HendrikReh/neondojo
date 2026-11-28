//! GNUbg Position ID and Match ID codecs. Every module reads and writes these
//! identifiers through this file.

use anyhow::{Context as _, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};

/// A field of GNUbg's 72-bit Match ID: `(offset, width)` in bits, least
/// significant bit first.
#[derive(Clone, Copy, Debug)]
pub struct Field(usize, usize);

impl Field {
    /// log2 of the cube value.
    pub const CUBE_LOG2: Self = Self(0, 4);
    /// 0 or 1 for the owner, 3 for a centered cube.
    pub const CUBE_OWNER: Self = Self(4, 2);
    /// The player on roll.
    pub const ON_ROLL: Self = Self(6, 1);
    pub const CRAWFORD: Self = Self(7, 1);
    /// 1 while a game is being played.
    pub const GAME_STATE: Self = Self(8, 3);
    /// The player who must make a decision.
    pub const TURN: Self = Self(11, 1);
    pub const DOUBLED: Self = Self(12, 1);
    pub const RESIGNED: Self = Self(13, 2);
    pub const DIE_0: Self = Self(15, 3);
    pub const DIE_1: Self = Self(18, 3);
    /// 0 for money play.
    pub const LENGTH: Self = Self(21, 15);
    pub const SCORE_0: Self = Self(36, 15);
    pub const SCORE_1: Self = Self(51, 15);
    /// Set when the Jacoby rule is off; GNUbg also sets it in match play.
    pub const NO_JACOBY: Self = Self(66, 1);
    /// Reserved bits after the GNUbg fields.
    pub const EXTENSION: Self = Self(67, 5);
}

pub struct MatchId([u8; 9]);

impl MatchId {
    pub fn decode(id: &str) -> Option<Self> {
        Some(Self(STANDARD_NO_PAD.decode(id).ok()?.try_into().ok()?))
    }

    pub fn encode(&self) -> String {
        STANDARD_NO_PAD.encode(self.0)
    }

    pub fn get(&self, Field(offset, width): Field) -> i32 {
        (0..width).fold(0, |value, i| {
            value | (i32::from((self.0[(offset + i) / 8] >> ((offset + i) % 8)) & 1) << i)
        })
    }

    pub fn set(&mut self, Field(offset, width): Field, value: i32) {
        for i in 0..width {
            let (byte, bit) = ((offset + i) / 8, (offset + i) % 8);
            self.0[byte] &= !(1 << bit);
            if (value >> i) & 1 != 0 {
                self.0[byte] |= 1 << bit;
            }
        }
    }

    /// Whether `doubler` (0 White, 1 Black) may legally offer a double in this
    /// match context: not Crawford, cube below the 4-bit Match ID ceiling
    /// (32768), centered or owned by `doubler`, and not dead for `doubler`'s
    /// score in match play.
    pub fn can_double(&self, doubler: i32) -> bool {
        if !matches!(doubler, 0 | 1) {
            return false;
        }
        let owner = self.get(Field::CUBE_OWNER);
        if self.get(Field::CRAWFORD) != 0 || (owner != 3 && owner != doubler) {
            return false;
        }
        let cube = 1 << self.get(Field::CUBE_LOG2);
        if cube >= 32768 {
            return false;
        }
        let length = self.get(Field::LENGTH);
        let score = self.get(if doubler == 0 {
            Field::SCORE_0
        } else {
            Field::SCORE_1
        });
        length == 0 || score + cube < length
    }
}

/// The player on roll in a Match ID.
pub fn on_roll(id: &str) -> Option<i32> {
    Some(MatchId::decode(id)?.get(Field::ON_ROLL))
}

/// The same Match ID before the dice are rolled.
pub fn hide_dice(id: &str) -> Option<String> {
    let mut id = MatchId::decode(id)?;
    id.set(Field::DIE_0, 0);
    id.set(Field::DIE_1, 0);
    Some(id.encode())
}

/// The native mistake browser displays a responder-oriented ID with a visual
/// offer bit. For evaluation, restore the responder-oriented decision ID.
pub fn without_offer_marker(id: &str) -> Option<String> {
    let mut id = MatchId::decode(id)?;
    id.set(Field::DOUBLED, 0);
    Some(id.encode())
}

/// Marks a responder-oriented decision ID as facing an offered double.
pub fn with_offer_marker(id: &str) -> Option<String> {
    let mut id = MatchId::decode(id)?;
    id.set(Field::DOUBLED, 1);
    Some(id.encode())
}

/// The same Match ID while `doubler` waits for the answer to a double. GNUbg's
/// player on roll stays the doubler; the opponent must decide.
pub fn with_offer_from(id: &str, doubler: i32) -> Option<String> {
    let mut id = MatchId::decode(id)?;
    id.set(Field::ON_ROLL, doubler);
    id.set(Field::TURN, 1 - doubler);
    id.set(Field::DOUBLED, 1);
    Some(id.encode())
}

/// GNUbg encodes the opponent's checkers first, then the player on roll.
pub fn position_id(board: &[i32; 26], player: i32) -> String {
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
pub fn decode_board(position_id: &str, match_id: &str) -> anyhow::Result<Vec<i32>> {
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
pub fn match_id(
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
        let board = crate::backgammon::initial_board();
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
    fn an_offered_double_keeps_the_doubler_on_roll() {
        for doubler in [0, 1] {
            for player in [0, 1] {
                let before = match_id(7, [2, 4], 2, doubler, false, false, player, [0, 0]);
                let offered = MatchId::decode(&with_offer_from(&before, doubler).unwrap()).unwrap();
                assert_eq!(offered.get(Field::ON_ROLL), doubler);
                assert_eq!(offered.get(Field::TURN), 1 - doubler);
                assert_eq!(offered.get(Field::DOUBLED), 1);
                let mut restored = MatchId::decode(&before).unwrap();
                for field in [Field::ON_ROLL, Field::TURN, Field::DOUBLED] {
                    restored.set(field, offered.get(field));
                }
                assert_eq!(
                    restored.encode(),
                    offered.encode(),
                    "no other field changes"
                );
            }
        }
    }

    #[test]
    fn invalid_ids_are_rejected() {
        assert!(MatchId::decode("AAAA").is_none());
        assert!(decode_board("AAAAAAAAAAAAAA", "AAAAAAAAAAAA").is_ok());
        assert!(decode_board("/////////////w", "AAAAAAAAAAAA").is_err());
        assert!(decode_board(OPENING, "AAAA").is_err());
    }

    #[test]
    fn can_double_enforces_ownership_crawford_ceiling_and_dead_cube() {
        let decode = |id: String| MatchId::decode(&id).unwrap();
        // Money play: centered or owned cube below 32768 may double; 32768 ceiling cannot.
        let money_center = decode(match_id(0, [0, 0], 1, -1, false, true, 0, [0, 0]));
        assert!(money_center.can_double(0));
        assert!(money_center.can_double(1));
        assert!(!money_center.can_double(2));
        let money_owned = decode(match_id(0, [0, 0], 2, 0, false, true, 0, [0, 0]));
        assert!(money_owned.can_double(0));
        assert!(!money_owned.can_double(1));
        let money_ceiling = decode(match_id(0, [0, 0], 32768, 0, false, true, 0, [0, 0]));
        assert!(!money_ceiling.can_double(0));

        // Crawford forbids both sides from doubling.
        let crawford = decode(match_id(7, [6, 3], 1, -1, true, false, 1, [0, 0]));
        assert!(!crawford.can_double(0));
        assert!(!crawford.can_double(1));

        // Post-Crawford 1-away: leader (score + cube == length) has a dead cube; trailer may double.
        let post_crawford = decode(match_id(7, [6, 3], 1, -1, false, false, 0, [0, 0]));
        assert!(!post_crawford.can_double(0));
        assert!(post_crawford.can_double(1));

        // 2-away with a 2-cube is dead (5 + 2 == 7); 3-away with a 2-cube is live (4 + 2 < 7).
        let dead_two = decode(match_id(7, [5, 2], 2, 0, false, false, 0, [0, 0]));
        assert!(!dead_two.can_double(0));
        let live_two = decode(match_id(7, [4, 2], 2, 0, false, false, 0, [0, 0]));
        assert!(live_two.can_double(0));
    }
}
