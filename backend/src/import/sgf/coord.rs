//! SGF checker coordinate encoding/decoding (`a..=x`, `y` = bar, `z` = off)
//! and mover-relative notation formatting.
pub(crate) use crate::backgammon::notation::{format_point, format_step};
use anyhow::{Result, bail};

/// Encodes a mover-relative point (`25` = bar, `1..=24` = points, `0` = off)
/// into its SGF coordinate character for `player` (`0` = White, `1` = Black).
pub(crate) fn encode_point(point: usize, player: i32) -> Result<char> {
    match point {
        0 => Ok('z'),
        25 => Ok('y'),
        1..=24 if player == 0 => Ok(char::from(b'a' + point as u8 - 1)),
        1..=24 if player == 1 => Ok(char::from(b'x' + 1 - point as u8)),
        _ => bail!("invalid SGF point {point} for player {player}"),
    }
}

/// Decodes an SGF coordinate byte (`a..=x`, `y` = bar, `z` = off) into a
/// mover-relative point (`25` = bar, `1..=24` = points, `0` = off).
pub(crate) fn decode_point(byte: u8, player: i32) -> Result<usize> {
    match byte {
        b'y' => Ok(25),
        b'z' => Ok(0),
        b'a'..=b'x' if player == 0 => Ok(usize::from(byte - b'a' + 1)),
        b'a'..=b'x' if player == 1 => Ok(usize::from(b'x' - byte + 1)),
        _ => bail!("invalid SGF move coordinate"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_all_points_for_both_players() {
        for player in [0, 1] {
            for point in 0..=25 {
                let ch = encode_point(point, player).unwrap();
                assert_eq!(decode_point(ch as u8, player).unwrap(), point);
            }
        }
        assert_eq!(encode_point(1, 0).unwrap(), 'a');
        assert_eq!(encode_point(24, 0).unwrap(), 'x');
        assert_eq!(encode_point(1, 1).unwrap(), 'x');
        assert_eq!(encode_point(24, 1).unwrap(), 'a');
        assert_eq!(encode_point(25, 0).unwrap(), 'y');
        assert_eq!(encode_point(0, 1).unwrap(), 'z');
        assert!(encode_point(26, 0).is_err());
        assert!(decode_point(b'A', 0).is_err());
    }
}
