//! Jellyfish MAT transcripts as GNUbg's reader splits them, and recovery of a
//! transcript whose moves GNUbg or the strict replay rejects.
//!
//! GNUbg reads a MAT file game by game and stops at the first move it cannot
//! read or play. In its last attempt it continues instead and applies the
//! faulty move, so its later diagnostics do not locate the fault. Recovery
//! therefore asks the complete MAT validation (GNUbg diagnostics and strict
//! SGF replay) about ever shorter prefixes of the transcript. The first move
//! cell whose prefix fails ends its game: the game keeps the moves before the
//! cell, has no result, and the next game starts from its own header. The
//! original file stays unchanged; only the text given to GNUbg is shortened.

mod native;
pub(crate) use native::{event_date, normalize_native_sgf};

use crate::model::{ImportWarning, ImportWarningKind, MatchGame};
use anyhow::{Context, Result, anyhow, bail};
use std::{future::Future, time::Duration};
use tokio::time::Instant;

/// Games a single transcript may lose to rejected moves before the import fails.
const MAX_TRUNCATED_GAMES: usize = 20;
/// GNUbg runs one recovery may start, including the first full attempt.
const MAX_VALIDATIONS: usize = 400;
/// Wall-clock budget of one recovery, from the first full attempt on. A GNUbg
/// validation takes about 0.1 s, so this covers the binary searches of
/// ordinary transcripts, while a malformed upload cannot hold one of the two
/// import permits for the minutes that `MAX_VALIDATIONS` runs could take.
const MAX_RECOVERY_TIME: Duration = Duration::from_secs(60);
const TOO_MANY_ERRORS: &str = "The MAT file has too many errors to locate";
const TOO_SLOW: &str = "Locating the errors in the MAT file took too long";
/// Characters of a rejected move kept in its warning.
const SOURCE_TEXT_LIMIT: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Left,
    Right,
}

/// One move cell: the text GNUbg passes to its MAT move parser.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Cell {
    line: usize,
    column: Column,
    /// Byte offset of the cell in its line.
    start: usize,
    /// Move number printed before `)` on the row.
    row: Option<i32>,
    text: String,
}

#[derive(Debug, Clone)]
struct Game {
    header: usize,
    score_line: Option<usize>,
    /// First line after this game.
    end: usize,
    cells: Vec<Cell>,
}

#[derive(Debug)]
struct Transcript<'a> {
    lines: Vec<&'a [u8]>,
    games: Vec<Game>,
}

/// GNUbg ends a MAT line at its first CR or LF.
fn content(line: &[u8]) -> &[u8] {
    let end = line
        .iter()
        .position(|b| matches!(b, b'\r' | b'\n'))
        .unwrap_or(line.len());
    &line[..end]
}

fn blank(line: &[u8]) -> bool {
    line.iter()
        .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// `%d %*1[Pp]oint %*1[Mm]atch%c`, the line GNUbg treats as the match length.
fn match_length_line(line: &[u8]) -> bool {
    let rest = line.trim_ascii_start();
    let rest = rest.strip_prefix(b"-").unwrap_or(rest);
    let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return false;
    }
    let rest = rest[digits..].trim_ascii_start();
    let Some((first, rest)) = rest.split_first() else {
        return false;
    };
    if !matches!(first, b'P' | b'p') {
        return false;
    }
    let Some(rest) = rest.strip_prefix(b"oint") else {
        return false;
    };
    let rest = rest.trim_ascii_start();
    let Some((first, rest)) = rest.split_first() else {
        return false;
    };
    matches!(first, b'M' | b'm') && rest.starts_with(b"atch") && rest.len() > 4
}

/// GNUbg's two-column split of a move row: the left cell starts after the row
/// number, the right cell two bytes before the second colon or, without one,
/// at the second space of a double space after column 15.
fn cells(line: usize, text: &[u8]) -> Vec<Cell> {
    let mut left_end = text.len();
    let mut right = None;
    let colons = text.iter().position(|b| *b == b':').and_then(|first| {
        text[first + 1..]
            .iter()
            .position(|b| *b == b':')
            .map(|second| first + 1 + second)
    });
    match colons {
        Some(second) if second > 3 => {
            left_end = second - 3;
            right = Some(second - 2);
        }
        _ => {
            if text.len() > 15
                && let Some(gap) = find(&text[15..], b"  ")
            {
                left_end = 15 + gap;
                right = Some(15 + gap + 1);
            }
        }
    }
    let number = text[..left_end].iter().position(|b| *b == b')');
    let left_start = number.map_or(0, |end| end + 1);
    let row = number.and_then(|end| {
        std::str::from_utf8(text[..end].trim_ascii())
            .ok()
            .and_then(|value| value.parse().ok())
    });
    let cell = |column, start: usize, end: usize| {
        let value = text[start..end].trim_ascii();
        (!value.is_empty()).then(|| Cell {
            line,
            column,
            start,
            row,
            text: String::from_utf8_lossy(value).into_owned(),
        })
    };
    [
        cell(Column::Left, left_start, left_end),
        right.and_then(|start| cell(Column::Right, start, text.len())),
    ]
    .into_iter()
    .flatten()
    .collect()
}

impl<'a> Transcript<'a> {
    /// The games GNUbg would read, or `None` without a match length line.
    fn parse(source: &'a [u8]) -> Option<Self> {
        let lines: Vec<&[u8]> = source.split_inclusive(|b| *b == b'\n').collect();
        let start = lines.iter().position(|line| match_length_line(line))? + 1;
        let headers: Vec<usize> = (start..lines.len())
            .filter(|&index| find(content(lines[index]), b"Game ").is_some())
            .collect();
        let games = headers
            .iter()
            .enumerate()
            .map(|(order, &header)| {
                let end = headers.get(order + 1).copied().unwrap_or(lines.len());
                let score_line = (header + 1..end).find(|&index| !blank(lines[index]));
                let cells = score_line
                    .map(|score| {
                        (score + 1..end)
                            .filter(|&index| !content(lines[index]).starts_with(b"; Set Pos="))
                            .flat_map(|index| cells(index, content(lines[index])))
                            .collect()
                    })
                    .unwrap_or_default();
                Game {
                    header,
                    score_line,
                    end,
                    cells,
                }
            })
            .collect();
        Some(Self { lines, games })
    }

    /// The transcript with each game keeping only its first `keep[game]`
    /// cells (all when `None`), and without any game after `last`.
    fn render(&self, keep: &[Option<usize>], last: Option<usize>) -> Vec<u8> {
        let first = self.games.first().map_or(self.lines.len(), |g| g.header);
        let mut out: Vec<u8> = self.lines[..first].concat();
        let last = last.unwrap_or(self.games.len().saturating_sub(1));
        for (index, game) in self.games.iter().enumerate().take(last + 1) {
            let cut = keep[index].and_then(|kept| game.cells.get(kept));
            let body_start = game.score_line.map_or(game.end, |score| score + 1);
            out.extend(self.lines[game.header..body_start].concat());
            let Some(cut) = cut else {
                out.extend(self.lines[body_start..game.end].concat());
                continue;
            };
            out.extend(self.lines[body_start..cut.line].concat());
            if cut.column == Column::Right {
                // Blanking keeps the left cell at its column and leaves GNUbg
                // an empty right cell, which it skips.
                let line = self.lines[cut.line];
                let end = content(line).len();
                out.extend(&line[..cut.start]);
                out.extend(std::iter::repeat_n(b' ', end - cut.start));
                out.extend(&line[end..]);
                if !line.ends_with(b"\n") {
                    out.push(b'\n');
                }
            }
        }
        out
    }
}

/// A game ended before a move the validation rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Truncation {
    game: usize,
    cell: Cell,
    unreadable: bool,
}

struct Located<'a> {
    sgf: String,
    transcript: Option<Transcript<'a>>,
    truncations: Vec<Truncation>,
}

/// Run one recovery validation unless the recovery has used up its runs or
/// its time, and cut it off at `deadline`. `Err` names the exhausted budget.
async fn attempt<F, Fut>(
    validate: &mut F,
    text: Vec<u8>,
    validations: &mut usize,
    deadline: Instant,
) -> std::result::Result<Result<String>, &'static str>
where
    F: FnMut(Vec<u8>) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    if *validations >= MAX_VALIDATIONS {
        return Err(TOO_MANY_ERRORS);
    }
    // `timeout_at` would still start a validation after the deadline.
    if Instant::now() >= deadline {
        return Err(TOO_SLOW);
    }
    *validations += 1;
    tokio::time::timeout_at(deadline, validate(text))
        .await
        .map_err(|_| TOO_SLOW)
}

/// Validate the transcript and, if it fails, end every game at its first
/// rejected move cell. Recovery stops with an error at `deadline`.
async fn locate<'a, F, Fut>(
    source: &'a [u8],
    mut validate: F,
    deadline: Instant,
) -> Result<Located<'a>>
where
    F: FnMut(Vec<u8>) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let transcript = Transcript::parse(source);
    let mut failure = match validate(source.to_vec()).await {
        Ok(sgf) => {
            return Ok(Located {
                sgf,
                transcript,
                truncations: Vec::new(),
            });
        }
        Err(error) => error,
    };
    let Some(transcript) = transcript.filter(|t| !t.games.is_empty()) else {
        return Err(failure);
    };
    let mut keep: Vec<Option<usize>> = vec![None; transcript.games.len()];
    let mut truncations = Vec::new();
    let mut validations = 1;
    let mut first_game = 0;
    loop {
        // Prefixes in reading order: (game, kept cells) for every game from
        // `first_game` on. Everything before them has already validated.
        let units: Vec<(usize, usize)> = (first_game..transcript.games.len())
            .flat_map(|game| (0..=transcript.games[game].cells.len()).map(move |kept| (game, kept)))
            .collect();
        // units[..low] validate; units[high..] and the full text do not.
        let (mut low, mut high) = (0, units.len());
        while low < high {
            let middle = low + (high - low) / 2;
            let (game, kept) = units[middle];
            let mut trial = keep.clone();
            trial[game] = (kept < transcript.games[game].cells.len()).then_some(kept);
            let text = transcript.render(&trial, Some(game));
            let outcome = match attempt(&mut validate, text, &mut validations, deadline).await {
                Ok(outcome) => outcome,
                Err(budget) => return Err(failure.context(budget)),
            };
            match outcome {
                Ok(_) => low = middle + 1,
                Err(error) => {
                    high = middle;
                    failure = error;
                }
            }
        }
        // The validation accepted the same text it rejected before.
        let &(game, kept) = units.get(high).ok_or_else(|| anyhow!("{failure:#}"))?;
        let faulty = &transcript.games[game];
        if kept == 0 {
            return Err(failure).with_context(|| {
                format!(
                    "MAT game {} cannot be read (line {})",
                    game + 1,
                    faulty.header + 1
                )
            });
        }
        let cell = faulty.cells[kept - 1].clone();
        let unreadable = format!("{failure:#}").contains("Unrecognised move");
        keep[game] = Some(kept - 1);
        truncations.push(Truncation {
            game,
            cell,
            unreadable,
        });
        if truncations.len() > MAX_TRUNCATED_GAMES {
            bail!("The MAT file has errors in more than {MAX_TRUNCATED_GAMES} games");
        }
        first_game = game + 1;
        let text = transcript.render(&keep, None);
        let outcome = match attempt(&mut validate, text, &mut validations, deadline).await {
            Ok(outcome) => outcome,
            Err(budget) => return Err(failure.context(budget)),
        };
        match outcome {
            Ok(sgf) => {
                return Ok(Located {
                    sgf,
                    transcript: Some(transcript),
                    truncations,
                });
            }
            Err(error) if first_game < transcript.games.len() => failure = error,
            Err(error) => return Err(error),
        }
    }
}

fn shorten(text: &str) -> String {
    match text.char_indices().nth(SOURCE_TEXT_LIMIT) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

/// Unfinished games and header scores that contradict the previous result.
fn continuity_warnings(
    games: &[MatchGame],
    score_lines: Option<&[Option<usize>]>,
    truncated: &[usize],
) -> Vec<ImportWarning> {
    let line = |index: usize| {
        score_lines
            .and_then(|lines| lines.get(index).copied().flatten())
            .and_then(|line| i32::try_from(line + 1).ok())
    };
    let mut warnings = Vec::new();
    for (index, pair) in games.windows(2).enumerate() {
        let [game, next] = pair else { continue };
        match &game.result {
            None if !truncated.contains(&index) => {
                let mut warning =
                    ImportWarning::at(ImportWarningKind::UnfinishedGame, game.game_no, None);
                warning.source_line = line(index);
                warnings.push(warning);
            }
            Some(result) if result.scores_after != [next.white_score, next.black_score] => {
                let mut warning =
                    ImportWarning::at(ImportWarningKind::ScoreMismatch, next.game_no, None);
                warning.source_line = line(index + 1);
                warning.expected_score = Some(result.scores_after);
                warning.recorded_score = Some([next.white_score, next.black_score]);
                warnings.push(warning);
            }
            _ => {}
        }
    }
    warnings
}

pub(crate) struct Recovered {
    pub sgf: String,
    pub warnings: Vec<ImportWarning>,
}

/// Import a MAT transcript through `validate`, ending games at rejected moves
/// and reporting every resolution as an import warning.
pub(crate) async fn recover<F, Fut>(source: &[u8], validate: F) -> Result<Recovered>
where
    F: FnMut(Vec<u8>) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let located = locate(source, validate, Instant::now() + MAX_RECOVERY_TIME).await?;
    let imported = super::parse_sgf(&located.sgf)?;
    // Lines only identify games when GNUbg kept every game of the transcript.
    let score_lines: Option<Vec<Option<usize>>> = located
        .transcript
        .as_ref()
        .filter(|t| t.games.len() == imported.games.len())
        .map(|t| t.games.iter().map(|game| game.score_line).collect());
    let truncated: Vec<usize> = located.truncations.iter().map(|t| t.game).collect();
    let mut warnings: Vec<ImportWarning> = located
        .truncations
        .iter()
        .map(|truncation| {
            let kind = if truncation.unreadable {
                ImportWarningKind::UnreadablePlayTruncated
            } else {
                ImportWarningKind::IllegalPlayTruncated
            };
            let game_no = imported
                .games
                .get(truncation.game)
                .map_or(truncation.game as i32 + 1, |game| game.game_no);
            let mut warning = ImportWarning::at(kind, game_no, truncation.cell.row);
            warning.source_line = i32::try_from(truncation.cell.line + 1).ok();
            warning.source_text = Some(shorten(&truncation.cell.text));
            warning
        })
        .collect();
    warnings.extend(continuity_warnings(
        &imported.games,
        score_lines.as_deref(),
        &truncated,
    ));
    warnings.sort_by_key(|warning| (warning.game_no, warning.source_line));
    Ok(Recovered {
        sgf: located.sgf,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GameResult, GameResultReason};

    const SOURCE: &str = concat!(
        "; [EventDate \"1988.01.01\"]\r\n",
        "\r\n",
        " 5 point match\r\n",
        "\r\n",
        " Game 1\r\n",
        " Alice : 0                      Bob : 0\r\n",
        "  1) 31: 8/5 6/5                 42: 8/4 6/4\r\n",
        "  2) 65: 24/13                    Doubles => 2\r\n",
        "  3)  Takes                      BAD: 13/7\r\n",
        "  4) 21: 13/11 6/5               33: 8/2(2)\r\n",
        "      Wins 1 point\r\n",
        "\r\n",
        " Game 2\r\n",
        " Alice : 0                      Bob : 1\r\n",
        "  1)                             61: 13/7 8/7\r\n",
        "  2) 64: 24/14                   55: 8/3(2) 6/1(2)\r\n",
    );

    fn transcript() -> Transcript<'static> {
        Transcript::parse(SOURCE.as_bytes()).unwrap()
    }

    #[test]
    fn rows_split_into_gnubg_cells_with_line_and_move_number() {
        let t = transcript();
        assert_eq!(t.games.len(), 2);
        let texts = |game: usize| -> Vec<(usize, Option<i32>, &str)> {
            t.games[game]
                .cells
                .iter()
                .map(|cell| (cell.line + 1, cell.row, cell.text.as_str()))
                .collect()
        };
        assert_eq!(
            texts(0),
            [
                (7, Some(1), "31: 8/5 6/5"),
                (7, Some(1), "42: 8/4 6/4"),
                (8, Some(2), "65: 24/13"),
                (8, Some(2), "Doubles => 2"),
                (9, Some(3), "Takes"),
                (9, Some(3), "BAD: 13/7"),
                (10, Some(4), "21: 13/11 6/5"),
                (10, Some(4), "33: 8/2(2)"),
                (11, None, "Wins 1 point"),
            ]
        );
        assert_eq!(
            texts(1),
            [
                (15, Some(1), "61: 13/7 8/7"),
                (16, Some(2), "64: 24/14"),
                (16, Some(2), "55: 8/3(2) 6/1(2)"),
            ]
        );
        assert_eq!(t.games[1].score_line, Some(13));
    }

    #[test]
    fn rendering_ends_a_game_at_a_cell_and_keeps_its_columns() {
        let t = transcript();
        let text = |keep: &[Option<usize>], last| String::from_utf8(t.render(keep, last)).unwrap();
        assert_eq!(text(&[None, None], None), SOURCE);
        // Ending before the right cell blanks it in place.
        let cut = text(&[Some(5), None], None);
        let row = |text: &str, prefix: &str| {
            text.split("\r\n")
                .find(|line| line.starts_with(prefix))
                .unwrap()
                .to_owned()
        };
        let original = row(SOURCE, "  3)");
        let blanked = row(&cut, "  3)");
        assert_eq!(blanked.trim_end(), "  3)  Takes");
        assert_eq!(blanked.len(), original.len());
        assert!(!cut.contains("BAD") && !cut.contains("Wins 1 point"));
        assert!(cut.ends_with("55: 8/3(2) 6/1(2)\r\n"));
        // Ending before a left cell drops its row; `last` drops later games.
        let prefix = text(&[Some(4), None], Some(0));
        assert!(
            prefix.ends_with("  2) 65: 24/13                    Doubles => 2\r\n"),
            "{prefix}"
        );
        assert!(!prefix.contains("Game 2"));
        assert!(
            text(&[Some(0), None], Some(0))
                .ends_with(" Alice : 0                      Bob : 0\r\n")
        );
    }

    /// Rejects any text whose kept cells contain one of `faults`, like the
    /// MAT validation rejects a prefix that ends with a faulty move.
    fn validator<'a>(
        faults: &'static [&'static str],
        calls: &'a std::cell::Cell<usize>,
    ) -> impl FnMut(Vec<u8>) -> std::future::Ready<Result<String>> + 'a {
        move |bytes| {
            calls.set(calls.get() + 1);
            let text = String::from_utf8(bytes).unwrap();
            std::future::ready(match faults.iter().find(|fault| text.contains(**fault)) {
                Some(fault) if fault.starts_with("BAD") => Err(anyhow!(
                    "GNUbg rejected the MAT file: Unrecognised move \"{fault}\""
                )),
                Some(fault) => Err(anyhow!("MAT game 1, checker ply 3: illegal {fault}")),
                None => Ok(text),
            })
        }
    }

    fn later() -> Instant {
        Instant::now() + MAX_RECOVERY_TIME
    }

    #[tokio::test]
    async fn recovery_stops_at_its_deadline() {
        // A deadline that has passed starts no recovery validation.
        let calls = std::cell::Cell::new(0);
        let error = locate(
            SOURCE.as_bytes(),
            validator(&["BAD: 13/7"], &calls),
            Instant::now(),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(calls.get(), 1, "only the first full attempt");
        assert_eq!(error.to_string(), TOO_SLOW);
        assert!(
            format!("{error:#}").contains("Unrecognised move"),
            "{error:#}"
        );

        // A validation still running at the deadline is abandoned, which
        // kills its GNUbg process (`kill_on_drop`).
        let calls = std::cell::Cell::new(0);
        let mut stalled = validator(&["BAD: 13/7"], &calls);
        let error = locate(
            SOURCE.as_bytes(),
            |text| {
                let first = calls.get() == 0;
                let outcome = stalled(text);
                async move {
                    if !first {
                        std::future::pending::<()>().await;
                    }
                    outcome.await
                }
            },
            Instant::now() + Duration::from_millis(50),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(calls.get(), 2);
        assert_eq!(error.to_string(), TOO_SLOW);
    }

    #[tokio::test]
    async fn a_valid_transcript_is_validated_once() {
        let calls = std::cell::Cell::new(0);
        let located = locate(SOURCE.as_bytes(), validator(&[], &calls), later())
            .await
            .unwrap();
        assert!(located.truncations.is_empty());
        assert_eq!(located.sgf, SOURCE);
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn each_faulty_game_ends_before_its_first_rejected_cell() {
        let calls = std::cell::Cell::new(0);
        let located = locate(
            SOURCE.as_bytes(),
            validator(&["BAD: 13/7", "55: 8/3(2)"], &calls),
            later(),
        )
        .await
        .unwrap();
        let found: Vec<(usize, usize, &str, bool)> = located
            .truncations
            .iter()
            .map(|t| (t.game, t.cell.line + 1, t.cell.text.as_str(), t.unreadable))
            .collect();
        assert_eq!(
            found,
            [
                (0, 9, "BAD: 13/7", true),
                (1, 16, "55: 8/3(2) 6/1(2)", false)
            ]
        );
        assert!(located.sgf.contains("  2) 64: 24/14"));
        assert!(!located.sgf.contains("55:") && !located.sgf.contains("BAD"));
        assert!(located.sgf.contains(" Game 2"));
        assert!(calls.get() <= 12, "{} validations", calls.get());
    }

    #[tokio::test]
    async fn a_rejected_header_or_text_without_games_keeps_the_error() {
        let calls = std::cell::Cell::new(0);
        let error = locate(SOURCE.as_bytes(), validator(&["Bob : 1"], &calls), later())
            .await
            .err()
            .unwrap();
        assert!(format!("{error:#}").contains("MAT game 2 cannot be read (line 13)"));
        let error = locate(b"not a match", validator(&["not"], &calls), later())
            .await
            .err()
            .unwrap();
        assert!(format!("{error:#}").contains("illegal not"));
    }

    fn game(game_no: i32, scores: [i32; 2], after: Option<[i32; 2]>) -> MatchGame {
        MatchGame {
            cube_enabled: Some(true),
            result: after.map(|scores_after| GameResult {
                winner: 0,
                points: 1,
                cube: 1,
                multiplier: None,
                reason: GameResultReason::Single,
                scores_after,
            }),
            game_no,
            position_count: 1,
            white_score: scores[0],
            black_score: scores[1],
            crawford: false,
        }
    }

    #[test]
    fn header_scores_are_checked_against_the_previous_result() {
        let games = [
            game(1, [0, 0], Some([1, 0])),
            game(2, [2, 0], None),
            game(3, [2, 0], None),
            game(4, [2, 1], Some([3, 1])),
            game(5, [3, 1], None),
        ];
        let lines = [Some(5), Some(13), Some(20), Some(30), Some(40)];
        let warnings = continuity_warnings(&games, Some(&lines), &[2]);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert_eq!(warnings[0].kind, ImportWarningKind::ScoreMismatch);
        assert_eq!(warnings[0].game_no, 2);
        assert_eq!(warnings[0].source_line, Some(14));
        assert_eq!(warnings[0].expected_score, Some([1, 0]));
        assert_eq!(warnings[0].recorded_score, Some([2, 0]));
        // Game 2 has no result in the file; game 3 was ended by the import.
        assert_eq!(warnings[1].kind, ImportWarningKind::UnfinishedGame);
        assert_eq!(warnings[1].game_no, 2);
    }

    #[test]
    fn the_match_length_line_follows_gnubgs_pattern() {
        for line in [" 41 point match\r\n", "5 Point Match \n", "0 point match\n"] {
            assert!(match_length_line(line.as_bytes()), "{line:?}");
        }
        for line in ["point match\n", "7 points match\n", "7 point match"] {
            assert!(!match_length_line(line.as_bytes()), "{line:?}");
        }
    }
}
