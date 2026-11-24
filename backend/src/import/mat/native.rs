//! The SGF GNUbg writes for a MAT transcript, made explicit and dated.

use crate::backgammon::{expand_joined_play, initial_board};
use crate::import::sgf::{one, parse_sgf, parse_trees, serialize_trees, setup};
use anyhow::{Context, Result, bail, ensure};

/// The match date a MAT transcript declares in a `; [EventDate "YYYY.MM.DD"]`
/// header comment. A transcript without a valid declaration has no date.
pub(crate) fn event_date(source: &[u8]) -> Option<chrono::NaiveDate> {
    String::from_utf8_lossy(source).lines().find_map(|line| {
        let comment = line
            .trim_start_matches('\u{feff}')
            .strip_prefix([';', '#'])?;
        let value = comment.trim_start().strip_prefix("[EventDate \"")?;
        let mut parts = value.split('"').next()?.trim().split('.');
        let year = parts.next()?.parse::<i32>().ok().filter(|year| *year > 0)?;
        let month = parts.next()?.parse::<u32>().ok()?;
        let day = parts.next()?.parse::<u32>().ok()?;
        if parts.next().is_some() {
            return None;
        }
        chrono::NaiveDate::from_ymd_opt(year, month, day)
    })
}

/// GNUbg's MAT reader can retain a joined overshoot bearoff in its SGF output
/// (for example 6/off with 52), despite expanding other joined checker runs.
/// Expand only the native MAT boundary; public SGF remains explicit and strict.
///
/// Without an `EventDate` header GNUbg writes the modification time of its
/// temporary input file as `DT`. Its `DT` is therefore replaced by the date
/// the transcript declares, and a missing date stays missing.
pub(crate) fn normalize_native_sgf(
    input: &str,
    declared_date: Option<chrono::NaiveDate>,
) -> Result<String> {
    let mut trees = parse_trees(input)?;
    for root in trees.iter_mut().filter_map(|nodes| nodes.first_mut()) {
        root.remove("DT");
    }
    if let (Some(date), Some(root)) = (
        declared_date,
        trees.first_mut().and_then(|nodes| nodes.first_mut()),
    ) {
        root.insert("DT".to_owned(), vec![date.format("%Y-%m-%d").to_string()]);
    }
    for (game, nodes) in trees.iter_mut().enumerate() {
        let mut board = initial_board();
        let mut ply = 0;
        for node in nodes {
            setup(node, &mut board)?;
            for (property, player) in [("W", 0), ("B", 1)] {
                let Some(value) = one(node, property)? else {
                    continue;
                };
                if matches!(value, "double" | "take" | "drop") {
                    continue;
                }
                let bytes = value.as_bytes();
                ensure!(
                    (2..=10).contains(&bytes.len())
                        && bytes.len().is_multiple_of(2)
                        && bytes[..2].iter().all(|d| (b'1'..=b'6').contains(d)),
                    "MAT: malformed native checker move {value}"
                );
                let dice = [i32::from(bytes[0] - b'0'), i32::from(bytes[1] - b'0')];
                let point = |c: u8| -> Result<usize> {
                    match c {
                        b'a'..=b'x' => Ok(if player == 0 {
                            usize::from(c - b'a' + 1)
                        } else {
                            usize::from(b'x' - c + 1)
                        }),
                        b'y' => Ok(25),
                        b'z' => Ok(0),
                        _ => bail!("MAT: invalid native checker coordinate"),
                    }
                };
                let runs = bytes[2..]
                    .chunks_exact(2)
                    .map(|pair| Ok((point(pair[0])?, point(pair[1])?)))
                    .collect::<Result<Vec<_>>>()?;
                let steps = expand_joined_play(&mut board, player, dice, &runs)
                    .with_context(|| format!("MAT game {}, checker ply {}", game + 1, ply + 1))?;
                let coordinate = |p: usize| match p {
                    25 => 'y',
                    0 => 'z',
                    _ if player == 0 => char::from(b'a' + p as u8 - 1),
                    _ => char::from(b'x' + 1 - p as u8),
                };
                let mut expanded = value[..2].to_owned();
                for (from, to) in steps {
                    expanded.push(coordinate(from));
                    expanded.push(coordinate(to));
                }
                node.insert(property.to_owned(), vec![expanded]);
                ply += 1;
            }
        }
    }
    let output = serialize_trees(&trees);
    // Check turns, cube state and match context as well as checker legality
    // before any native artifact is returned for persistence.
    parse_sgf(&output)?;
    Ok(output)
}
