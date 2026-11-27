//! The only absolute/GNU-ID to mover-relative Sage context conversion.
use crate::backgammon::ids::{Field, MatchId};
use anyhow::{Context as _, Result, ensure};
use serde::Serialize;

/// The match equity tables in the supported NeonGammon wheels cover up to 25-away.
pub(crate) const MAX_AWAY: i32 = 25;

pub(crate) fn ensure_supported_away(away1: i32, away2: i32) -> Result<()> {
    ensure!(
        away1.max(away2) <= MAX_AWAY,
        "NeonGammon cannot evaluate {away1}-away vs {away2}-away (supported up to {MAX_AWAY}-away per player)"
    );
    Ok(())
}

pub(crate) fn validate_match_id_away(match_id: &str) -> Result<()> {
    let id = MatchId::decode(match_id).context("Invalid GNUbg Match ID")?;
    let length = id.get(Field::LENGTH);
    if length > 0 {
        let player = id.get(Field::ON_ROLL);
        let scores = [id.get(Field::SCORE_0), id.get(Field::SCORE_1)];
        let away1 = length - scores[player as usize];
        let away2 = length - scores[(1 - player) as usize];
        ensure_supported_away(away1, away2)?;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub(super) struct Context {
    pub cube_value: i32,
    pub cube_owner: &'static str,
    pub away1: i32,
    pub away2: i32,
    pub is_crawford: bool,
    pub jacoby: bool,
    #[serde(skip)]
    pub player: i32,
    #[serde(skip)]
    pub dice: [i32; 2],
    #[serde(skip)]
    pub pending_double: bool,
}

impl Context {
    pub fn decode(match_id: &str) -> Result<Self> {
        let id = MatchId::decode(match_id).context("Invalid GNUbg Match ID")?;
        let player = id.get(Field::ON_ROLL);
        let owner = id.get(Field::CUBE_OWNER);
        ensure!(owner != 2, "Invalid cube owner");
        ensure!(
            id.get(Field::GAME_STATE) == 1,
            "NeonGammon requires an active game"
        );
        ensure!(
            id.get(Field::RESIGNED) == 0,
            "Resignation context is unsupported"
        );
        let length = id.get(Field::LENGTH);
        let scores = [id.get(Field::SCORE_0), id.get(Field::SCORE_1)];
        ensure!(
            length == 0 || scores.iter().all(|score| *score < length),
            "Match has already ended"
        );
        let away1 = if length > 0 {
            length - scores[player as usize]
        } else {
            0
        };
        let away2 = if length > 0 {
            length - scores[(1 - player) as usize]
        } else {
            0
        };
        ensure_supported_away(away1, away2)?;
        let dice = [id.get(Field::DIE_0), id.get(Field::DIE_1)];
        ensure!(dice.iter().all(|die| *die <= 6), "Invalid Match ID dice");
        Ok(Self {
            cube_value: 1 << id.get(Field::CUBE_LOG2),
            cube_owner: if owner == 3 {
                "centered"
            } else if owner == player {
                "player"
            } else {
                "opponent"
            },
            away1,
            away2,
            is_crawford: id.get(Field::CRAWFORD) == 1,
            jacoby: length == 0 && id.get(Field::NO_JACOBY) == 0,
            player,
            dice,
            pending_double: id.get(Field::DOUBLED) == 1,
        })
    }

    pub fn board(&self, position_id: &str, match_id: &str) -> Result<[i32; 26]> {
        let absolute: [i32; 26] = crate::backgammon::ids::decode_board(position_id, match_id)?
            .try_into()
            .ok()
            .context("Invalid board length")?;
        Ok(to_sage(&absolute, self.player))
    }
}

pub(super) fn to_sage(absolute: &[i32; 26], player: i32) -> [i32; 26] {
    let mut board = [0; 26];
    for point in 1..25 {
        board[point] = if player == 0 {
            absolute[point]
        } else {
            -absolute[25 - point]
        };
    }
    board[25] = if player == 0 {
        absolute[25]
    } else {
        -absolute[0]
    };
    board[0] = if player == 0 {
        -absolute[0]
    } else {
        absolute[25]
    };
    board
}

pub(super) fn from_sage(board: &[i32; 26], player: i32) -> Result<[i32; 26]> {
    ensure!(board[0] >= 0 && board[25] >= 0, "Invalid Sage bar signs");
    let mut absolute = [0; 26];
    for point in 1..25 {
        absolute[point] = if player == 0 {
            board[point]
        } else {
            board[25 - point]
                .checked_neg()
                .context("Invalid Sage checker count")?
        };
    }
    absolute[25] = if player == 0 { board[25] } else { board[0] };
    absolute[0] = if player == 0 { -board[0] } else { -board[25] };
    crate::backgammon::board::validate(&absolute)?;
    Ok(absolute)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backgammon::ids;
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};

    #[test]
    fn asymmetric_bars_and_both_players_preserve_absolute_board() {
        let mut absolute = [0; 26];
        absolute[0] = -2;
        absolute[25] = 1;
        absolute[6] = 14;
        absolute[24] = -13;
        let white = to_sage(&absolute, 0);
        assert_eq!((white[0], white[25], white[6], white[24]), (2, 1, 14, -13));
        let black = to_sage(&absolute, 1);
        assert_eq!((black[0], black[25], black[19], black[1]), (1, 2, -14, 13));
        assert_eq!(from_sage(&white, 0).unwrap(), absolute);
        assert_eq!(from_sage(&black, 1).unwrap(), absolute);
        for player in [0, 1] {
            let mid = ids::match_id(7, [2, 4], 2, 0, false, false, player, [3, 1]);
            let context = Context::decode(&mid).unwrap();
            assert_eq!(
                context
                    .board(&ids::position_id(&absolute, player), &mid)
                    .unwrap(),
                to_sage(&absolute, player)
            );
            assert_eq!(context.away1, if player == 0 { 5 } else { 3 });
            assert_eq!(context.away2, if player == 0 { 3 } else { 5 });
            assert_eq!(
                context.cube_owner,
                if player == 0 { "player" } else { "opponent" }
            );
        }
    }

    #[test]
    fn pending_offer_uses_original_doubler_not_response_actor() {
        let mid = ids::match_id(5, [1, 3], 2, 0, false, false, 0, [0, 0]);
        let mut bytes = STANDARD_NO_PAD.decode(mid).unwrap();
        bytes[1] |= (1 << 3) | (1 << 4);
        let context = Context::decode(&STANDARD_NO_PAD.encode(bytes)).unwrap();
        assert_eq!(context.player, 0);
        assert!(context.pending_double);
        assert_eq!(context.away1, 4);
        assert_eq!(context.cube_owner, "player");
    }

    #[test]
    fn rejects_away_scores_above_25_and_accepts_up_to_25_and_money() {
        for (length, scores, player, expected_ok) in [
            (0, [0, 0], 0, true),
            (25, [0, 0], 0, true),
            (31, [6, 6], 0, true),
            (31, [6, 6], 1, true),
            (31, [5, 6], 0, false),
            (31, [5, 6], 1, false),
            (31, [6, 5], 0, false),
            (31, [5, 5], 0, false),
        ] {
            let mid = ids::match_id(length, scores, 1, 3, false, false, player, [3, 1]);
            let decoded = Context::decode(&mid);
            let away_check = validate_match_id_away(&mid);
            assert_eq!(
                decoded.is_ok(),
                expected_ok,
                "length={length} scores={scores:?} player={player}: {decoded:?}"
            );
            assert_eq!(
                away_check.is_ok(),
                expected_ok,
                "length={length} scores={scores:?} player={player}: {away_check:?}"
            );
            if !expected_ok {
                assert!(
                    decoded.unwrap_err().to_string().contains("25-away"),
                    "expected 25-away message"
                );
            }
        }
    }
}
