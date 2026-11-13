//! OGXM v2 match import. Translate format coordinates and events to SGF and
//! leave board, move legality and match IDs to the shared SGF replay.
mod analysis;
mod wire;

use super::match_context::MatchContext;
use super::sgf::{Node, serialize_trees};
use anyhow::{Context, Result, bail, ensure};
use wire::{Reader, Sections, has};

pub(crate) const SGF_APPLICATION: &str = "BGTutor:OGXM2";

const DICE: [[u8; 2]; 21] = [
    [1, 1],
    [1, 2],
    [1, 3],
    [1, 4],
    [1, 5],
    [1, 6],
    [2, 2],
    [2, 3],
    [2, 4],
    [2, 5],
    [2, 6],
    [3, 3],
    [3, 4],
    [3, 5],
    [3, 6],
    [4, 4],
    [4, 5],
    [4, 6],
    [5, 5],
    [5, 6],
    [6, 6],
];

struct Match<'a> {
    names: [&'a str; 2],
    length: u32,
    rules: u32,
    cube_limit: u32,
    start: [u32; 2],
    final_score: Option<[u32; 2]>,
    crawford_before_start: bool,
    context: MatchContext,
}

fn pair(r: &mut Reader<'_>) -> Result<[u32; 2]> {
    Ok([r.var()?, r.var()?])
}

impl<'a> Match<'a> {
    fn read(input: &'a [u8]) -> Result<Self> {
        let mut r = Reader::new(input);
        let (mask, mut body) = r.record()?;
        r.finish(0, 0)?;
        let length = body.var()?;
        ensure!(length <= 32767, "OGXM match length exceeds GNUbg range");
        ensure!(
            body.var()? == 0,
            "Only standard backgammon OGXM matches are supported"
        );
        let white = if has(mask, 0) {
            body.string()?
        } else {
            "White"
        };
        let black = if has(mask, 1) {
            body.string()?
        } else {
            "Black"
        };
        let rules = if has(mask, 2) { body.var()? } else { 0 };
        ensure!(
            rules & !3 == 0,
            "Unsupported OGXM rules (beavers, raccoons, automatic doubles or unknown rules)"
        );
        let cube_limit = if has(mask, 3) { body.var()? } else { 32768 };
        ensure!(cube_limit.is_power_of_two(), "Invalid OGXM cube limit");
        let start = if has(mask, 4) {
            pair(&mut body)?
        } else {
            [0; 2]
        };
        let final_score = has(mask, 5).then(|| pair(&mut body)).transpose()?;
        if has(mask, 6) {
            ensure!((1..=3).contains(&body.var()?), "Invalid OGXM match result");
        }
        if has(mask, 7) {
            body.var()?;
        }
        let start_time = has(mask, 8).then(|| body.leb(10)).transpose()?;
        if has(mask, 9) {
            body.leb(10)?;
        }
        if has(mask, 10) {
            body.var()?;
        }
        let event = has(mask, 12).then(|| body.string()).transpose()?;
        let year = has(mask, 13).then(|| body.var()).transpose()?;
        let precision = has(mask, 14).then(|| body.var()).transpose()?;
        let stage = has(mask, 15).then(|| body.string()).transpose()?;
        let round = has(mask, 16).then(|| body.var()).transpose()?;
        if has(mask, 17) {
            body.string()?;
        }
        let city = has(mask, 18).then(|| body.string()).transpose()?;
        let country = has(mask, 19).then(|| body.string()).transpose()?;
        if has(mask, 20) {
            body.string()?;
        }
        let site = has(mask, 21).then(|| body.string()).transpose()?;
        if has(mask, 22) {
            body.string()?;
        }
        for bit in [23, 24] {
            if has(mask, bit) {
                body.record()?;
            }
        }
        body.finish(mask, 26)?;
        let date = start_time
            .filter(|_| matches!(precision, None | Some(0)))
            .and_then(|time| i64::try_from(time).ok())
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|time| time.date_naive());
        let event =
            event.map(|name| year.map_or_else(|| name.to_owned(), |y| format!("{name} {y}")));
        let round = stage
            .map(str::to_owned)
            .or_else(|| round.map(|n| n.to_string()));
        let place = match (city, country) {
            (Some(city), Some(country)) => Some(format!("{city}, {country}")),
            (one, other) => one.or(other).or(site).map(str::to_owned),
        };
        Ok(Self {
            names: [white, black],
            length,
            rules,
            cube_limit,
            start,
            final_score,
            crawford_before_start: has(mask, 11),
            context: MatchContext::new(date, event.as_deref(), round.as_deref(), place.as_deref()),
        })
    }
}

fn property(node: &mut Node, name: &str, value: impl Into<String>) {
    node.insert(name.to_owned(), vec![value.into()]);
}

fn color(seat: usize) -> &'static str {
    ["W", "B"][seat]
}

// OGXM White moves up in absolute coordinates; SGF White moves down.
// The mover-relative points in either color are 25=bar, 0=off.
fn relative_point(from: u8, seat: usize) -> u8 {
    if seat == 0 { 25 - from } else { from }
}

fn steps_to_sgf(
    steps: &[u8],
    seat: usize,
    dice: [u8; 2],
    clipped_bearoffs: bool,
) -> Result<String> {
    let mut available = [0; 7];
    for die in dice {
        available[usize::from(die)] += if dice[0] == dice[1] { 2 } else { 1 };
    }
    let coordinate = |point| match point {
        0 => 'z',
        25 => 'y',
        _ if seat == 0 => char::from(b'a' + point - 1),
        _ => char::from(b'y' - point),
    };
    let mut out = String::new();
    for &step in steps {
        let (from, pips) = (step & 31, step >> 5);
        ensure!(
            from <= 25 && (1..=6).contains(&pips),
            "Invalid OGXM checker step"
        );
        let from = relative_point(from, seat);
        ensure!(from != 0, "OGXM checker step starts off the board");
        if !clipped_bearoffs || pips != from {
            ensure!(
                available[usize::from(pips)] > 0,
                "OGXM checker step does not match the roll"
            );
            available[usize::from(pips)] -= 1;
        }
        out.push(coordinate(from));
        out.push(coordinate(from.saturating_sub(pips)));
    }
    Ok(out)
}

struct Ply<'a> {
    seat: usize,
    action: u8,
    dice: Option<[u8; 2]>,
    steps: &'a [u8],
    resign: Option<u32>,
}

impl<'a> Ply<'a> {
    fn read(r: &mut Reader<'a>) -> Result<Self> {
        let head = r.byte()?;
        let action = head & 63;
        let seat = usize::from((head >> 6) & 1);
        let dice = DICE.get(usize::from(action)).copied();
        let mut steps = &[][..];
        if let Some([a, b]) = dice {
            let max = if a == b { 4 } else { 2 };
            let data = r.remaining();
            for count in 0..max {
                if r.byte()? == 0 {
                    steps = &data[..count];
                    break;
                }
                steps = &data[..=count];
            }
        }
        let resign = if head & 128 != 0 {
            let (mask, mut extras) = r.record()?;
            ensure!(
                matches!(action, 27 | 28) && mask == 2,
                "Unsupported OGXM ply extras for action {action}"
            );
            let value = extras.var()?;
            extras.finish(mask, 9)?;
            ensure!((1..=3).contains(&value), "Invalid OGXM resignation factor");
            Some(value)
        } else {
            ensure!(
                !matches!(action, 27 | 28),
                "Missing OGXM resignation factor"
            );
            None
        };
        Ok(Self {
            seat,
            action,
            dice,
            steps,
            resign,
        })
    }

    fn marker(&self) -> bool {
        matches!(self.action, 24..=26 | 30)
    }
}

struct Game<'a> {
    award: Option<(usize, u32)>,
    last: bool,
    plies: Vec<Ply<'a>>,
}

impl<'a> Game<'a> {
    fn read(data: &'a [u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let (mask, mut body) = r.record()?;
        ensure!(
            mask & 0x78 == 0,
            "OGXM custom setups and automatic doubles are unsupported"
        );
        let award = match (has(mask, 0), has(mask, 1)) {
            (true, true) => {
                let winner = body.var()? as usize;
                let points = body.var()?;
                ensure!(
                    winner <= 1 && (1..=32767).contains(&points),
                    "Invalid OGXM game award"
                );
                Some((winner, points))
            }
            (false, false) => None,
            _ => bail!("Unsupported OGXM settlement or incomplete game award"),
        };
        if has(mask, 7) {
            ensure!(
                matches!(body.var()?, 0..=2 | 4),
                "Unsupported OGXM game termination"
            );
            ensure!(award.is_some(), "Missing OGXM game award");
        }
        body.finish(mask, 8)?;
        let mut plies = Vec::new();
        while !r.remaining().is_empty() {
            ensure!(plies.len() < 1500, "Too many plies in OGXM game");
            plies.push(Ply::read(&mut r).with_context(|| format!("OGXM ply {}", plies.len() + 1))?);
        }
        Ok(Self {
            award,
            last: has(mask, 2),
            plies,
        })
    }
}

#[derive(Clone, Copy)]
struct Placement {
    game: usize,
    node: Option<usize>,
    seat: usize,
    action: u8,
    dice: Option<[u8; 2]>,
    checker_index: Option<usize>,
}

pub fn ogxm_to_sgf(input: &[u8]) -> Result<String> {
    let sections = Sections::read(input)?;
    let data = Match::read(sections.metadata)?;
    let mut scores = data.start;
    let mut crawford_seen = data.crawford_before_start;
    let mut placements = Vec::new();
    let mut checker_count = 0;
    let mut trees = Vec::new();
    for (game_no, payload) in sections.games.iter().enumerate() {
        let game = Game::read(payload).with_context(|| format!("OGXM game {}", game_no + 1))?;
        ensure!(
            !game.last || game_no + 1 == sections.games.len(),
            "OGXM has games after the last game"
        );
        ensure!(
            scores
                .iter()
                .all(|&s| s <= 32767 && (data.length == 0 || s < data.length)),
            "Invalid starting score for OGXM game {}",
            game_no + 1
        );
        ensure!(
            placements.len() + game.plies.len() <= 100_000,
            "Too many OGXM plies"
        );
        let crawford = data.length > 0
            && data.rules & 1 != 0
            && !crawford_seen
            && scores.contains(&(data.length - 1));
        crawford_seen |= crawford;
        let mut root = Node::new();
        for (key, value) in [
            ("FF", "4"),
            ("GM", "6"),
            ("CA", "UTF-8"),
            ("AP", SGF_APPLICATION),
            ("PW", data.names[0]),
            ("PB", data.names[1]),
        ] {
            property(&mut root, key, value);
        }
        root.insert(
            "MI".into(),
            vec![
                format!("length:{}", data.length),
                format!("game:{game_no}"),
                format!("ws:{}", scores[0]),
                format!("bs:{}", scores[1]),
            ],
        );
        if crawford {
            property(&mut root, "RU", "Crawford:CrawfordGame");
        } else if data.length > 0 && data.rules & 1 != 0 {
            property(&mut root, "RU", "Crawford");
        } else if data.length == 0 && data.rules & 2 != 0 {
            property(&mut root, "RU", "Jacoby");
        }
        if game_no == 0 {
            for (key, value) in data.context.sgf_properties() {
                property(&mut root, key, value);
            }
        }
        let mut nodes = vec![root];
        let mut cube = 1;
        let mut pending = false;
        let mut rolled = false;
        let mut ended = false;
        let mut resignation = false;
        let mut events = game.plies.iter().peekable();
        while let Some(ply) = events.next() {
            let mut placement = Placement {
                game: game_no,
                node: None,
                seat: ply.seat,
                action: ply.action,
                dice: ply.dice,
                checker_index: None,
            };
            let value = if ply.marker() {
                ensure!(ply.seat == 0, "Invalid OGXM marker seat");
                None
            } else {
                ensure!(!ended, "OGXM actions after game end");
                match ply.action {
                    0..=20 => {
                        let [a, b] = ply.dice.context("Missing OGXM dice")?;
                        if !rolled {
                            ensure!(a != b, "OGXM opening roll cannot be a double");
                        }
                        rolled = true;
                        if ply.steps.is_empty()
                            && events.peek().is_some_and(|p| matches!(p.action, 27..=29))
                        {
                            placement.dice = None; // A rolled but unplayed turn is not a checker decision.
                            None
                        } else {
                            let steps = steps_to_sgf(ply.steps, ply.seat, [a, b], false)?;
                            placement.checker_index = Some(checker_count);
                            checker_count += 1;
                            Some(format!("{a}{b}{steps}"))
                        }
                    }
                    21 => {
                        ensure!(rolled && cube < data.cube_limit, "Invalid OGXM cube offer");
                        pending = true;
                        Some("double".into())
                    }
                    22 => {
                        ensure!(pending && cube < 32768, "Invalid OGXM cube acceptance");
                        pending = false;
                        cube *= 2;
                        Some("take".into())
                    }
                    23 => {
                        ensure!(
                            pending && game.award.is_some(),
                            "OGXM drop without an offer or game award"
                        );
                        ended = true;
                        Some("drop".into())
                    }
                    27..=29 => {
                        ensure!(
                            game.award.is_some_and(|(winner, _)| winner != ply.seat),
                            "Invalid OGXM ending seat or missing game award"
                        );
                        // SGF's R suffix cannot represent a resignation with an outstanding double.
                        resignation = ply.resign.is_some() && !pending;
                        ended = true;
                        None
                    }
                    action => bail!("Unsupported OGXM action {action}"),
                }
            };
            if let Some(value) = value {
                let mut node = Node::new();
                property(&mut node, color(ply.seat), value);
                placement.node = Some(nodes.len());
                nodes.push(node);
            }
            placements.push(placement);
        }
        if let Some((winner, points)) = game.award {
            let suffix = if resignation { "R" } else { "" };
            property(
                &mut nodes[0],
                "RE",
                format!("{}+{points}{suffix}", color(winner)),
            );
            let score = scores[winner]
                .checked_add(points)
                .context("OGXM score overflow")?;
            scores[winner] = if data.length == 0 {
                score
            } else {
                score.min(data.length)
            };
        } else {
            ensure!(
                game_no + 1 == sections.games.len(),
                "Unfinished OGXM game before the last game"
            );
        }
        trees.push(nodes);
    }
    ensure!(
        data.final_score.is_none_or(|expected| expected == scores),
        "OGXM final score mismatch"
    );

    let played_sgf = serialize_trees(&trees);
    let imported = super::parse_sgf(&played_sgf).context("OGXM played-line validation failed")?;
    for (index, (game, payload)) in imported.games.iter().zip(&sections.games).enumerate() {
        // A missing award on a terminal board would make SGF infer a result
        // the file never declared, invalidating the next score or result.
        let (mask, _) = Reader::new(payload).record()?;
        ensure!(
            has(mask, 0) == game.result.is_some(),
            "OGXM game {} has inconsistent result",
            index + 1
        );
    }
    if sections.analyses.is_empty() {
        return Ok(played_sgf);
    }
    analysis::attach(&sections, &placements, &imported.positions, &mut trees)?;
    let sgf = serialize_trees(&trees);
    ensure!(
        sgf.len() <= super::sgf::MAX_SGF_BYTES,
        "OGXM canonical SGF exceeds 32 MiB"
    );
    Ok(sgf)
}
