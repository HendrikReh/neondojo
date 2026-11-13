use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};

pub(crate) struct MatchId([u8; 9]);

impl MatchId {
    pub(crate) fn decode(id: &str) -> Option<Self> {
        Some(Self(STANDARD_NO_PAD.decode(id).ok()?.try_into().ok()?))
    }

    pub(crate) fn bits(&self, offset: usize, width: usize) -> i32 {
        (0..width).fold(0, |value, i| {
            value | (i32::from((self.0[(offset + i) / 8] >> ((offset + i) % 8)) & 1) << i)
        })
    }
}

pub(crate) fn hide_dice(id: &str) -> Option<String> {
    let mut bytes = MatchId::decode(id)?.0;
    bytes[1] &= 0x7f;
    bytes[2] &= 0xe0;
    Some(STANDARD_NO_PAD.encode(bytes))
}

/// The native mistake browser displays a responder-oriented ID with a visual
/// offer bit. For evaluation, restore the responder-oriented decision ID.
pub(crate) fn without_offer_marker(id: &str) -> Option<String> {
    let mut bytes = MatchId::decode(id)?.0;
    bytes[1] &= !0x10;
    Some(STANDARD_NO_PAD.encode(bytes))
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
    let mut bytes = [0u8; 9];
    let mut set = |offset: usize, width: usize, value: i32| {
        for i in 0..width {
            if (value >> i) & 1 != 0 {
                bytes[(offset + i) / 8] |= 1 << ((offset + i) % 8);
            }
        }
    };
    set(0, 4, cube.trailing_zeros() as i32);
    set(4, 2, owner & 3);
    set(6, 1, player);
    set(7, 1, i32::from(crawford));
    set(8, 3, 1); // GAME_PLAYING
    set(11, 1, player);
    set(15, 3, dice[0].max(dice[1]));
    set(18, 3, dice[0].min(dice[1]));
    set(21, 15, length);
    set(36, 15, scores[0]);
    set(51, 15, scores[1]);
    set(66, 1, i32::from(!jacoby));
    STANDARD_NO_PAD.encode(bytes)
}
