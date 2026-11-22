//! Encoder for XG2 `.xg` files from a stored, validated SGF match.
//!
//! The output follows the layouts in XG_format.pas and ZLIBArchive.pas (see
//! tests/fixtures/README.md): TSaveRec records in `temp.xg`, the first and last
//! record in `temp.xgi`, a compressed ZLIBArchive registry and a RichGame
//! header without thumbnail. Field conventions that the prose specification
//! leaves open follow the official XG-written WC2014 file. Only the played line
//! is written; every analysis field carries XG's "not analysed" markers.
use super::{
    ids::{Field, MatchId},
    sgf::{self, GameContext, SourceMove},
    xg::{
        ARCHIVE_FOOTER_BYTES, MATCH_MAGIC, RECORD_BYTES, REGISTRY_ENTRY_BYTES, RICH_HEADER_BYTES,
        crc32,
    },
};
use crate::model::{GameResult, GameResultReason, MatchGame};
use anyhow::{Context, Result, bail, ensure};
use chrono::NaiveDate;
use flate2::{Compression, write::ZlibEncoder};
use std::io::Write;
use uuid::Uuid;

/// Library metadata written into the XG match header. It may differ from the
/// SGF root when the user edited the match details.
#[derive(Debug, Clone, Copy)]
pub struct XgExportMetadata<'a> {
    /// Stable source of the RichGame GUID and the XG game ID.
    pub match_id: Uuid,
    pub played_on: Option<NaiveDate>,
    pub event: Option<&'a str>,
    pub round: Option<&'a str>,
    pub place: Option<&'a str>,
}

/// `SaveFileVersion` of XG 2.10. Every field written here exists in it.
const SAVE_VERSION: i32 = 30;
/// XG's MatchLength for an unlimited money session.
const MONEY_SESSION: i32 = 99999;
/// XG's error value for a decision that was never analysed.
const NOT_ANALYSED: f64 = -1000.0;
/// XG writes the dice of a turn that was not rolled (a drop or a resignation)
/// as the two characters "77".
const NOT_ROLLED: [i32; 2] = [7, 7];

const HEADER_MATCH: u8 = 0;
const HEADER_GAME: u8 = 1;
const CUBE: u8 = 2;
const MOVE: u8 = 3;
const FOOTER_GAME: u8 = 4;
const FOOTER_MATCH: u8 = 5;

/// One TSaveRec. Offsets are those of the packed Delphi variant records.
struct Record([u8; RECORD_BYTES]);

impl Record {
    fn new(kind: u8) -> Self {
        let mut bytes = [0; RECORD_BYTES];
        bytes[8] = kind;
        Self(bytes)
    }
    fn int(&mut self, at: usize, value: i32) {
        self.0[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn double(&mut self, at: usize, value: f64) {
        self.0[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }
    fn flag(&mut self, at: usize, value: bool) {
        self.0[at] = u8::from(value);
    }
    /// A PositionEngine of 26 ShortInts.
    fn board(&mut self, at: usize, board: &[i32; 26]) {
        for (i, count) in board.iter().enumerate() {
            self.0[at + i] = *count as i8 as u8;
        }
    }
    /// A Delphi `string[limit]`: length byte, then Latin-1 bytes.
    fn short_string(&mut self, at: usize, limit: usize, text: &str) {
        let bytes: Vec<u8> = text
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .take(limit)
            .collect();
        self.0[at] = bytes.len() as u8;
        self.0[at + 1..at + 1 + bytes.len()].copy_from_slice(&bytes);
    }
    /// A TShortUnicodeString: at most 128 UTF-16 units and a terminating #0.
    fn unicode(&mut self, at: usize, text: &str) {
        write_utf16(&mut self.0[at..at + 258], text);
    }
}

/// Zero-terminated UTF-16LE that never splits a surrogate pair.
fn write_utf16(target: &mut [u8], text: &str) {
    let capacity = target.len() / 2 - 1;
    let mut units = Vec::new();
    for c in text.chars() {
        let mut buffer = [0u16; 2];
        let encoded = c.encode_utf16(&mut buffer);
        if units.len() + encoded.len() > capacity {
            break;
        }
        units.extend_from_slice(encoded);
    }
    for (i, unit) in units.iter().enumerate() {
        target[i * 2..i * 2 + 2].copy_from_slice(&unit.to_le_bytes());
    }
}

/// XG player sign: player 1 (+1) is SGF White, player 2 (-1) is SGF Black.
fn xg_player(player: i32) -> i32 {
    if player == 0 { 1 } else { -1 }
}

/// CubeA/CubeB: log2(value), signed by the absolute owner, zero when centered.
fn cube_exponent(cube: i32, owner: i32) -> Result<i32> {
    ensure!(
        cube > 0 && (cube as u32).is_power_of_two(),
        "XG export: invalid cube value {cube}"
    );
    let exponent = cube.trailing_zeros() as i32;
    match owner {
        -1 => {
            ensure!(
                cube == 1,
                "XG export: a centered cube above 1 cannot be written to XG"
            );
            Ok(0)
        }
        0 | 1 => Ok(exponent * xg_player(owner)),
        _ => bail!("XG export: invalid cube owner {owner}"),
    }
}

/// Cube value and owner (-1 centered, 0 White, 1 Black) before the event.
fn cube_before(event: &SourceMove) -> Result<(i32, i32)> {
    let id = MatchId::decode(&event.match_id).context("XG export: invalid stored Match ID")?;
    let owner = match id.get(Field::CUBE_OWNER) {
        3 => -1,
        owner @ (0 | 1) => owner,
        _ => bail!("XG export: invalid cube owner in stored Match ID"),
    };
    Ok((1 << id.get(Field::CUBE_LOG2), owner))
}

fn board_of(values: &[i32]) -> Result<[i32; 26]> {
    values
        .try_into()
        .context("XG export: stored board must have 26 points")
}

/// The mover-relative steps of a recorded checker play, in their recorded
/// order when the stored notation lists them. Pairs are (from, to) with the
/// bar at 25 and off at 0.
fn recorded_pairs(notation: &str) -> Option<Vec<(usize, usize)>> {
    if notation == "Cannot move" {
        return Some(Vec::new());
    }
    let point = |label: &str| match label.trim_end_matches('*') {
        "bar" => Some(25),
        "off" => Some(0),
        n => n.parse::<usize>().ok().filter(|p| (1..=24).contains(p)),
    };
    notation
        .split_whitespace()
        .map(|step| {
            let (from, to) = step.split_once('/')?;
            Some((point(from)?, point(to)?))
        })
        .collect()
}

/// XG Moves[]: zero-based From/To pairs with -1 as terminator. Bearing off
/// stores From minus the die used, which is negative.
fn xg_moves(event: &SourceMove) -> Result<[i32; 8]> {
    let board = board_of(&event.board)?;
    let after = board_of(&event.after_board)?;
    let dice = event.dice;
    let paths = sgf::checker_paths(&board, event.player, dice)?;
    let mut moves = [-1; 8];
    if paths.is_empty() {
        ensure!(
            board == after,
            "XG export: a forced pass cannot change the board (game {}, move {})",
            event.game_no,
            event.move_no
        );
        return Ok(moves);
    }
    let reaching: Vec<_> = paths.iter().filter(|path| path.board == after).collect();
    let recorded = recorded_pairs(&event.notation);
    // Bear-offs can leave the die choice open. XG uses the larger die for a
    // final checker and, when both dice bear off from one point, plays the
    // smaller die first.
    let path = reaching
        .iter()
        .filter(|path| {
            recorded.as_ref().is_some_and(|pairs| {
                pairs.len() == path.steps.len()
                    && pairs
                        .iter()
                        .zip(&path.steps)
                        .all(|(&(from, to), &(f, t, _))| from == f && to == t)
            })
        })
        .min_by_key(|path| {
            let dice: Vec<i32> = path.steps.iter().map(|step| step.2).collect();
            (std::cmp::Reverse(dice.iter().sum::<i32>()), dice)
        })
        .or_else(|| reaching.first())
        .with_context(|| {
            format!(
                "XG export: no legal play reaches the recorded position (game {}, move {})",
                event.game_no, event.move_no
            )
        })?;
    for (i, &(from, to, die)) in path.steps.iter().enumerate() {
        let from = from as i32 - 1;
        moves[i * 2] = from;
        moves[i * 2 + 1] = if to == 0 { from - die } else { to as i32 - 1 };
    }
    Ok(moves)
}

/// The rules a session declares in the SGF roots.
struct SessionRules {
    crawford: bool,
    cube_in_use: bool,
}

fn session_rules(trees: &[Vec<sgf::Node>], length: i32) -> SessionRules {
    let tokens: Vec<&str> = trees
        .iter()
        .filter_map(|nodes| nodes.first()?.get("RU")?.first())
        .flat_map(|rules| rules.split(':').map(str::trim))
        .collect();
    let declared = trees
        .iter()
        .any(|nodes| nodes.first().is_some_and(|root| root.contains_key("RU")));
    SessionRules {
        // Sources without any RU property (BGF, OGXM, MAT conversions) play
        // point matches under the standard Crawford rule.
        crawford: length > 0
            && (!declared
                || tokens
                    .iter()
                    .any(|t| matches!(*t, "Crawford" | "CrawfordGame"))),
        cube_in_use: !tokens.contains(&"NoCube"),
    }
}

enum CubeAction {
    /// The player rolls. XG marks the decision -2 when the cube is not
    /// available and 0 when the player could have doubled.
    Roll {
        available: bool,
    },
    Double {
        take: bool,
    },
    Resign,
}

struct Turn<'a> {
    context: &'a GameContext,
    player: i32,
    board: [i32; 26],
    cube: i32,
    owner: i32,
}

fn cube_record(turn: &Turn, action: CubeAction, dice: [i32; 2]) -> Result<Record> {
    let mut r = Record::new(CUBE);
    r.int(12, xg_player(turn.player));
    let (double, take) = match action {
        CubeAction::Roll { available } => (if available { 0 } else { -2 }, -1),
        CubeAction::Double { take } => (1, i32::from(take)),
        CubeAction::Resign => (-1, -1),
    };
    r.int(16, double);
    r.int(20, take);
    r.int(24, -1); // BeaverR
    r.int(28, -1); // RaccoonR
    r.int(32, cube_exponent(turn.cube, turn.owner)?);
    r.board(36, &turn.board);
    // DoubleD: an unanalysed EngineStructDoubleAction has FlagDouble -100.
    r.0[64 + 56..64 + 58].copy_from_slice(&(-100i16).to_le_bytes());
    r.double(200, NOT_ANALYSED); // ErrCube
    let rolled = format!("{}{}", dice[0], dice[1]);
    r.short_string(208, 2, &rolled);
    r.double(216, NOT_ANALYSED); // ErrTake
    r.int(224, -1); // RolloutindexD
    r.int(232, -1); // AnalyzeC
    r.double(240, NOT_ANALYSED); // ErrBeaver
    r.double(248, NOT_ANALYSED); // ErrRaccoon
    r.int(256, -1); // AnalyzeCR
    r.0[264] = 0xff; // TutorCube
    r.0[265] = 0xff; // TutorTake
    r.double(272, NOT_ANALYSED); // ErrTutorCube
    r.double(280, NOT_ANALYSED); // ErrTutorTake
    r.int(292, -1); // CommentCube
    Ok(r)
}

fn move_record(turn: &Turn, event: &SourceMove, edited: bool) -> Result<Record> {
    let mut r = Record::new(MOVE);
    // PositionI is absolute; PositionEnd is from the mover's perspective.
    r.board(9, &turn.board);
    r.board(
        35,
        &sgf::local_board(&board_of(&event.after_board)?, event.player),
    );
    r.int(64, xg_player(event.player));
    for (i, value) in xg_moves(event)?.iter().enumerate() {
        r.int(68 + i * 4, *value);
    }
    r.int(100, event.dice[0]);
    r.int(104, event.dice[1]);
    r.int(108, cube_exponent(turn.cube, turn.owner)?);
    r.flag(2308, true); // Played
    r.double(2312, NOT_ANALYSED); // ErrMove
    r.double(2320, NOT_ANALYSED); // ErrLuck
    for i in 0..32 {
        r.int(2344 + i * 4, -1); // RolloutindexM
    }
    r.int(2472, -1); // AnalyzeM
    r.int(2476, -1); // AnalyzeL
    r.0[2510] = 0xff; // Tutor
    r.double(2512, NOT_ANALYSED); // ErrTutorMove
    r.int(2524, -1); // CommentMove
    r.flag(2528, edited);
    Ok(r)
}

/// XG may not double in the opening roll, in the Crawford game, with an
/// opponent-owned cube, or when a doubled game cannot win the player more
/// than the points they still need.
fn can_double(turn: &Turn, rules: &SessionRules, opening: bool) -> bool {
    let away = turn.context.length - turn.context.scores[turn.player as usize];
    rules.cube_in_use
        && !opening
        && !turn.context.crawford
        && (turn.owner == -1 || turn.owner == turn.player)
        && (turn.context.length == 0 || away > turn.cube)
}

/// XG termination codes: 0 drop, 1–3 single/gammon/backgammon, +100 resign.
fn termination(result: &GameResult) -> i32 {
    match result.reason {
        GameResultReason::Drop => 0,
        GameResultReason::Single => 1,
        GameResultReason::Gammon => 2,
        GameResultReason::Backgammon => 3,
        GameResultReason::Resignation | GameResultReason::Recorded => {
            100 + result.multiplier.unwrap_or(1).clamp(1, 3)
        }
    }
}

fn write_game(
    out: &mut Vec<Record>,
    context: &GameContext,
    game: &MatchGame,
    events: &[SourceMove],
    rules: &SessionRules,
    next_crawford: bool,
) -> Result<()> {
    let initial = sgf::initial_board();
    let start_board = match events.first() {
        Some(event) => board_of(&event.board)?,
        None => initial,
    };
    let start_cube = match events.first() {
        Some(event) => cube_before(event)?,
        None => (1, -1),
    };
    let custom_start = start_board != initial || start_cube != (1, -1);
    let mut header = Record::new(HEADER_GAME);
    header.int(12, context.scores[0]);
    header.int(16, context.scores[1]);
    header.flag(20, context.crawford);
    header.board(21, &start_board);
    header.int(48, game.game_no);
    header.flag(52, game.result.is_none()); // InProgress
    header.int(56, -1); // CommentHeaderGame
    header.int(60, -1); // CommentFooterGame
    out.push(header);

    let mut expected = start_board;
    let mut first_checker = true;
    let mut rolled_after_take = None;
    let mut index = 0;
    while let Some(event) = events.get(index) {
        index += 1;
        let (cube, owner) = cube_before(event)?;
        let turn = Turn {
            context,
            player: event.player,
            board: board_of(&event.board)?,
            cube,
            owner,
        };
        match (event.kind, event.notation.as_str()) {
            ("checker", _) => {
                // XG marks a hand-entered position as edited: the start of a
                // custom game, or a setup that interrupts the replayed line.
                let edited = turn.board != expected || (first_checker && custom_start);
                if rolled_after_take.take() != Some(event.player) {
                    let opening = first_checker && !custom_start;
                    let available = can_double(&turn, rules, opening);
                    out.push(cube_record(
                        &turn,
                        CubeAction::Roll { available },
                        event.dice,
                    )?);
                }
                out.push(move_record(&turn, event, edited)?);
                expected = board_of(&event.after_board)?;
                first_checker = false;
            }
            ("cube", "Double") => {
                let response = events
                    .get(index)
                    .filter(|r| r.kind == "cube" && r.player != event.player)
                    .with_context(|| {
                        format!(
                            "XG export: double without a take or drop in game {}",
                            game.game_no
                        )
                    })?;
                index += 1;
                let take = match response.notation.as_str() {
                    "Take" => true,
                    "Drop" => false,
                    other => bail!("XG export: unsupported cube response {other}"),
                };
                // The double record carries the doubler's next roll.
                let rolled = events
                    .get(index)
                    .filter(|next| take && next.kind == "checker" && next.player == event.player)
                    .map_or(NOT_ROLLED, |next| next.dice);
                if take && rolled != NOT_ROLLED {
                    rolled_after_take = Some(event.player);
                }
                out.push(cube_record(&turn, CubeAction::Double { take }, rolled)?);
            }
            ("resign", _) => out.push(cube_record(&turn, CubeAction::Resign, NOT_ROLLED)?),
            (kind, notation) => bail!("XG export: unsupported event {kind} {notation}"),
        }
    }

    if let Some(result) = &game.result {
        let mut footer = Record::new(FOOTER_GAME);
        footer.int(12, result.scores_after[0]);
        footer.int(16, result.scores_after[1]);
        footer.flag(20, next_crawford);
        footer.int(24, xg_player(result.winner));
        footer.int(28, result.points);
        footer.int(32, termination(result));
        footer.double(40, NOT_ANALYSED); // ErrResign
        footer.double(48, NOT_ANALYSED); // ErrTakeResign
        out.push(footer);
    }
    Ok(())
}

/// Days since 1899-12-30, the integral part of a Delphi TDateTime.
fn delphi_date(date: Option<NaiveDate>) -> f64 {
    let epoch = NaiveDate::from_ymd_opt(1899, 12, 30).expect("valid Delphi epoch");
    date.map_or(0.0, |date| (date - epoch).num_days() as f64)
}

/// XG's positive game ID, stable for a library match.
fn game_id(match_id: Uuid) -> i32 {
    let bytes = match_id.as_bytes();
    let id = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) & i32::MAX;
    id.max(1)
}

fn match_header(
    players: &[String; 2],
    length: i32,
    rules: &SessionRules,
    jacoby: bool,
    cube_limit: i32,
    meta: &XgExportMetadata,
) -> Record {
    let event = meta.event.unwrap_or_default();
    let place = meta.place.unwrap_or_default();
    let round = meta.round.unwrap_or_default();
    let mut r = Record::new(HEADER_MATCH);
    r.short_string(9, 40, &players[0]);
    r.short_string(50, 40, &players[1]);
    r.int(92, if length == 0 { MONEY_SESSION } else { length });
    r.int(96, 0); // Variation: standard backgammon
    r.flag(100, rules.crawford);
    r.flag(101, jacoby && length == 0);
    r.double(104, 1600.0); // Elo1
    r.double(112, 1600.0); // Elo2
    r.double(128, delphi_date(meta.played_on));
    r.short_string(136, 128, event);
    r.int(268, game_id(meta.match_id));
    r.int(272, -1); // CompLevel1: human
    r.int(276, -1); // CompLevel2: human
    r.short_string(283, 128, place);
    r.short_string(417, 128, round);
    r.int(548, 1); // Invert
    r.int(552, SAVE_VERSION);
    r.int(556, MATCH_MAGIC as i32);
    r.int(576, -1); // CommentHeaderMatch
    r.int(580, -1); // CommentFooterMatch
    r.int(608, -1); // SiteId: none
    r.int(612, cube_limit);
    r.unicode(622, event);
    r.unicode(880, &players[0]);
    r.unicode(1138, &players[1]);
    r.unicode(1396, place);
    r.unicode(1654, round);
    r
}

/// Convert a stored, validated SGF match into a binary XG2 file.
pub fn sgf_to_xg(sgf: &str, meta: &XgExportMetadata) -> Result<Vec<u8>> {
    let trees = sgf::parse_trees(sgf)?;
    let (imported, contexts, events) = sgf::parse_sgf_events(sgf)?;
    ensure!(!contexts.is_empty(), "XG export: the match has no games");
    let length = imported.match_length;
    ensure!(
        length < MONEY_SESSION,
        "XG export: match length {length} is outside XG's range"
    );
    let rules = session_rules(&trees, length);
    let jacoby = contexts.iter().any(|context| context.jacoby);
    // CubeLimit is an exponent; XG's default allows cubes up to 1024.
    let mut cube_limit = 10;
    for event in &events {
        cube_limit = cube_limit.max(cube_before(event)?.0.trailing_zeros() as i32 + 1);
    }

    let mut records = vec![match_header(
        &imported.players,
        length,
        &rules,
        jacoby,
        cube_limit,
        meta,
    )];
    let mut remaining = events.as_slice();
    for (index, (context, game)) in contexts.iter().zip(&imported.games).enumerate() {
        let count = remaining
            .iter()
            .take_while(|event| event.game_no == game.game_no)
            .count();
        let (game_events, rest) = remaining.split_at(count);
        remaining = rest;
        let next_crawford = contexts.get(index + 1).is_some_and(|next| next.crawford);
        write_game(
            &mut records,
            context,
            game,
            game_events,
            &rules,
            next_crawford,
        )?;
    }
    ensure!(
        remaining.is_empty(),
        "XG export: events outside the recorded games"
    );
    if let Some(result) = imported.games.last().and_then(|game| game.result.as_ref())
        && length > 0
        && result.scores_after.iter().any(|score| *score >= length)
    {
        let mut footer = Record::new(FOOTER_MATCH);
        footer.int(12, result.scores_after[0]);
        footer.int(16, result.scores_after[1]);
        footer.int(20, xg_player(result.winner));
        records.push(footer);
    }

    let stream: Vec<u8> = records.iter().flat_map(|r| r.0).collect();
    let mut index = records[0].0.to_vec();
    index.extend_from_slice(&records[records.len() - 1].0);
    container(&stream, &index, &imported.players, length, meta)
}

fn compress(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}

/// RichGame header, ZLIBArchive payload, compressed registry and footer.
fn container(
    stream: &[u8],
    index: &[u8],
    players: &[String; 2],
    length: i32,
    meta: &XgExportMetadata,
) -> Result<Vec<u8>> {
    let mut archive = Vec::new();
    let mut registry = Vec::new();
    for (name, data) in [("temp.xg", stream), ("temp.xgi", index)] {
        let compressed = compress(data)?;
        // ZLIBArchive stores an entry unchanged when compression does not help.
        let (payload, status, level) = if compressed.len() < data.len() {
            (compressed.as_slice(), 0, 2)
        } else {
            (data, 1, 0)
        };
        let mut entry = [0u8; REGISTRY_ENTRY_BYTES];
        entry[0] = name.len() as u8;
        entry[1..=name.len()].copy_from_slice(name.as_bytes());
        let size = |n: usize| i32::try_from(n).context("XG export: archive exceeds 2 GiB");
        entry[512..516].copy_from_slice(&size(data.len())?.to_le_bytes());
        entry[516..520].copy_from_slice(&size(payload.len())?.to_le_bytes());
        entry[520..524].copy_from_slice(&size(archive.len())?.to_le_bytes());
        entry[524..528].copy_from_slice(&crc32(data).to_le_bytes());
        entry[528] = status;
        entry[529] = level;
        registry.extend_from_slice(&entry);
        archive.extend_from_slice(payload);
    }
    let registry = compress(&registry)?;
    let payload_size = archive.len();
    archive.extend_from_slice(&registry);
    let mut footer = [0u8; ARCHIVE_FOOTER_BYTES];
    footer[0..4].copy_from_slice(&crc32(&archive).to_le_bytes());
    footer[4..8].copy_from_slice(&2i32.to_le_bytes()); // FileCount
    footer[8..12].copy_from_slice(&1i32.to_le_bytes()); // Version
    footer[12..16].copy_from_slice(&(registry.len() as i32).to_le_bytes());
    footer[16..20].copy_from_slice(&(payload_size as i32).to_le_bytes());
    footer[20] = 1; // CompressedRegistry
    archive.extend_from_slice(&footer);

    let mut out = vec![0u8; RICH_HEADER_BYTES];
    out[0..4].copy_from_slice(b"RGMH");
    out[4..8].copy_from_slice(&1u32.to_le_bytes());
    out[8..12].copy_from_slice(&(RICH_HEADER_BYTES as u32).to_le_bytes());
    // liThumbnailOffset and dwThumbnailSize stay zero: no thumbnail.
    out[24..40].copy_from_slice(&meta.match_id.to_bytes_le());
    let session = if length == 0 {
        "money session".to_owned()
    } else {
        format!("{length} point match")
    };
    let mut level = Vec::new();
    if let Some(event) = meta.event.filter(|e| !e.is_empty()) {
        level.push(format!("Event: {event}"));
    }
    if let Some(round) = meta.round.filter(|r| !r.is_empty()) {
        level.push(format!("Round: {round}"));
    }
    let names = [
        meta.place
            .filter(|p| !p.is_empty())
            .map(|p| format!("Played on {p}"))
            .unwrap_or_default(),
        format!("{} vs. {}, {session}", players[0], players[1]),
        level.join(" "),
        String::new(),
    ];
    for (i, text) in names.iter().enumerate() {
        let at = 40 + i * 2048;
        write_utf16(&mut out[at..at + 2048], text);
    }
    out.extend_from_slice(&archive);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::xg::{convert_xg, main_records, xg_to_sgf};

    const WC2014: &[u8] = include_bytes!("../../tests/fixtures/WC2014.xg");

    fn meta(sgf: &str) -> (Uuid, crate::databases::SourceMetadata) {
        (
            Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef),
            crate::databases::source_metadata(sgf),
        )
    }
    fn export(sgf: &str) -> Vec<u8> {
        let (id, m) = meta(sgf);
        sgf_to_xg(
            sgf,
            &XgExportMetadata {
                match_id: id,
                played_on: m.played_on,
                event: m.tournament.as_deref(),
                round: m.round.as_deref(),
                place: m.place.as_deref(),
            },
        )
        .unwrap()
    }
    fn records(xg: &[u8]) -> Vec<Vec<u8>> {
        main_records(xg)
            .unwrap()
            .chunks_exact(RECORD_BYTES)
            .map(<[u8]>::to_vec)
            .collect()
    }
    fn int(r: &[u8], at: usize) -> i32 {
        i32::from_le_bytes(r[at..at + 4].try_into().unwrap())
    }
    fn ints(r: &[u8], at: usize, n: usize) -> Vec<i32> {
        (0..n).map(|i| int(r, at + i * 4)).collect()
    }
    fn bytes(r: &[u8], at: usize, n: usize) -> &[u8] {
        &r[at..at + n]
    }
    /// The played-line fields of each record type, as XG itself wrote them.
    fn played_line(r: &[u8]) -> Vec<Vec<i32>> {
        let b = |at, n| {
            bytes(r, at, n)
                .iter()
                .map(|x| i32::from(*x as i8))
                .collect()
        };
        match r[8] {
            // XG leaves stale memory after each #0 terminator, so Unicode
            // fields compare up to it. XG 2.19 wrote save version 31.
            HEADER_MATCH => [622, 880, 1138, 1396, 1654]
                .map(|at| {
                    bytes(r, at, 258)
                        .chunks_exact(2)
                        .map(|u| i32::from(u16::from_le_bytes([u[0], u[1]])))
                        .take_while(|u| *u != 0)
                        .collect()
                })
                .into_iter()
                .chain([
                    ints(r, 92, 2),
                    b(100, 4),
                    vec![f64::from_le_bytes(r[128..136].try_into().unwrap()).floor() as i32],
                    b(9, 82),
                    b(136, 129),
                    b(283, 129),
                    b(417, 129),
                    vec![int(r, 556)],
                ])
                .collect(),
            // XG left PosInit empty in the first game and wrote the opening
            // setup in later ones; both mean the standard start.
            HEADER_GAME => {
                let mut start = bytes(r, 21, 26).to_vec();
                if start.iter().all(|n| *n == 0) {
                    start = sgf::initial_board().map(|n| n as u8).to_vec();
                }
                vec![
                    ints(r, 12, 2),
                    b(20, 1),
                    start.iter().map(|x| i32::from(*x as i8)).collect(),
                    vec![int(r, 48)],
                    b(52, 1),
                ]
            }
            CUBE => vec![ints(r, 12, 3), vec![int(r, 32)], b(36, 26), b(208, 3)],
            MOVE => vec![
                b(9, 52),
                ints(r, 64, 1),
                ints(r, 68, 8),
                ints(r, 100, 3),
                b(2308, 1),
                ints(r, 2480, 1),
                b(2528, 1),
            ],
            FOOTER_GAME => vec![ints(r, 12, 2), b(20, 1), ints(r, 24, 3)],
            FOOTER_MATCH => vec![ints(r, 12, 3)],
            kind => panic!("unexpected record type {kind}"),
        }
    }

    #[test]
    fn official_xg_match_round_trips_to_the_records_xg_wrote() {
        let original = records(WC2014);
        let sgf = xg_to_sgf(WC2014).unwrap();
        let exported = records(&export(&sgf));
        assert_eq!(exported.len(), original.len());
        for (index, (a, b)) in exported.iter().zip(&original).enumerate() {
            assert_eq!(a[8], b[8], "record {index} type");
            assert_eq!(
                played_line(a),
                played_line(b),
                "record {index} (type {}) differs from the XG original",
                a[8]
            );
        }
        // Analysis is never invented: every decision is marked unanalysed.
        assert!(exported.iter().all(|r| match r[8] {
            CUBE => int(r, 232) == -1,
            MOVE => int(r, 120) == 0 && int(r, 2472) == -1,
            _ => true,
        }));
    }

    #[test]
    fn exported_matches_reimport_with_identical_positions_and_results() {
        let sgf = xg_to_sgf(WC2014).unwrap();
        let reimported = convert_xg(&export(&sgf)).unwrap();
        assert!(reimported.warnings.is_empty());
        let expected = crate::import::parse_sgf(&sgf).unwrap();
        let actual = crate::import::parse_sgf(&reimported.sgf).unwrap();
        assert_eq!(actual.players, expected.players);
        assert_eq!(actual.match_length, 11);
        assert_eq!(actual.positions.len(), 460);
        for (a, b) in actual.positions.iter().zip(&expected.positions) {
            assert_eq!(
                (&a.position_id, &a.match_id, a.game_no, a.ply, a.dice),
                (&b.position_id, &b.match_id, b.game_no, b.ply, b.dice)
            );
            assert_eq!(a.notation, b.notation);
        }
        for (a, b) in actual.games.iter().zip(&expected.games) {
            assert_eq!(a.result, b.result);
        }
        let (a, b) = (
            crate::databases::source_metadata(&reimported.sgf),
            crate::databases::source_metadata(&sgf),
        );
        assert_eq!(
            (a.played_on, a.tournament, a.round, a.place),
            (b.played_on, b.tournament, b.round, b.place)
        );
        assert_eq!(
            (a.result_state, a.white_score, a.black_score),
            (b.result_state, b.white_score, b.black_score)
        );
    }

    #[test]
    fn archive_holds_the_match_stream_and_its_index() {
        let xg = export(&xg_to_sgf(WC2014).unwrap());
        assert_eq!(&xg[..4], b"RGMH");
        assert_eq!(u32::from_le_bytes(xg[20..24].try_into().unwrap()), 0);
        let archive = &xg[RICH_HEADER_BYTES..];
        let footer = &archive[archive.len() - ARCHIVE_FOOTER_BYTES..];
        assert_eq!(int(footer, 4), 2);
        let registry_at = int(footer, 16) as usize;
        let registry = &archive[registry_at..archive.len() - ARCHIVE_FOOTER_BYTES];
        let mut decoder = flate2::read::ZlibDecoder::new(registry);
        let mut entries = Vec::new();
        std::io::Read::read_to_end(&mut decoder, &mut entries).unwrap();
        let entry = |i: usize| &entries[i * REGISTRY_ENTRY_BYTES..(i + 1) * REGISTRY_ENTRY_BYTES];
        assert_eq!(&entry(0)[..8], b"\x07temp.xg");
        assert_eq!(&entry(1)[..9], b"\x08temp.xgi");
        let start = int(entry(1), 520) as usize;
        let size = int(entry(1), 516) as usize;
        let mut index = Vec::new();
        std::io::Read::read_to_end(
            &mut flate2::read::ZlibDecoder::new(&archive[start..start + size]),
            &mut index,
        )
        .unwrap();
        let stream = main_records(&xg).unwrap();
        assert_eq!(&index[..RECORD_BYTES], &stream[..RECORD_BYTES]);
        assert_eq!(
            &index[RECORD_BYTES..],
            &stream[stream.len() - RECORD_BYTES..]
        );
    }

    #[test]
    fn money_drop_writes_jacoby_session_without_match_footer() {
        let sgf = "(;FF[4]GM[6]CA[UTF-8]PW[Ann]PB[Bob]MI[length:0][game:0][ws:0][bs:0]RU[Jacoby]\n;W[31hefe];B[double];W[drop])\n";
        let xg = export(sgf);
        let r = records(&xg);
        let kinds: Vec<u8> = r.iter().map(|r| r[8]).collect();
        assert_eq!(
            kinds,
            [HEADER_MATCH, HEADER_GAME, CUBE, MOVE, CUBE, FOOTER_GAME]
        );
        assert_eq!(int(&r[0], 92), MONEY_SESSION);
        assert_eq!(r[0][101], 1, "Jacoby");
        assert_eq!(r[0][100], 0, "no Crawford rule in money play");
        // The opening roll cannot be doubled; Black doubles and White drops.
        assert_eq!(ints(&r[2], 12, 3), [1, -2, -1]);
        assert_eq!(ints(&r[4], 12, 3), [-1, 1, 0]);
        assert_eq!(bytes(&r[4], 208, 3), b"\x0277");
        assert_eq!(ints(&r[5], 24, 3), [-1, 1, 0]);
        let reimported = convert_xg(&xg).unwrap();
        let parsed = crate::import::parse_sgf(&reimported.sgf).unwrap();
        assert_eq!(parsed.positions.len(), 1);
        assert!(reimported.sgf.contains("Jacoby"));
    }

    #[test]
    fn resignation_and_custom_start_survive_reimport() {
        // A bear-off race from a hand-entered position, resigned for a gammon.
        let sgf = "(;FF[4]GM[6]CA[UTF-8]PW[Ann]PB[Bob]MI[length:5][game:0][ws:0][bs:0]RU[Crawford]RE[W+2R]\n;AE[a:y]AW[a][a][b]AB[x][x][w][l]PL[W]\n;W[21bzaz])\n";
        let xg = export(sgf);
        let r = records(&xg);
        let kinds: Vec<u8> = r.iter().map(|r| r[8]).collect();
        assert_eq!(
            kinds,
            [HEADER_MATCH, HEADER_GAME, CUBE, MOVE, CUBE, FOOTER_GAME]
        );
        assert_ne!(bytes(&r[1], 21, 26), [0; 26], "PosInit holds the setup");
        assert_eq!(r[3][2528], 1, "the hand-entered start is marked edited");
        assert_eq!(ints(&r[2], 12, 2), [1, 0], "the cube is available");
        assert_eq!(
            ints(&r[3], 68, 4),
            [1, -1, 0, -1],
            "bear-offs store From-die"
        );
        assert_eq!(ints(&r[4], 12, 2), [-1, -1], "Black resigns on roll");
        assert_eq!(ints(&r[5], 12, 2), [2, 0]);
        assert_eq!(ints(&r[5], 24, 3), [1, 2, 102]);
        let reimported = convert_xg(&xg).unwrap();
        assert_eq!(
            reimported.warnings.first().map(|w| w.kind),
            Some(crate::model::ImportWarningKind::CustomStartPosition)
        );
        let expected = crate::import::parse_sgf(sgf).unwrap();
        let actual = crate::import::parse_sgf(&reimported.sgf).unwrap();
        assert_eq!(
            actual.positions[0].position_id,
            expected.positions[0].position_id
        );
        assert_eq!(actual.positions[0].match_id, expected.positions[0].match_id);
        assert_eq!(actual.games[0].result, expected.games[0].result);
    }

    #[test]
    fn names_keep_unicode_and_fall_back_to_latin1_for_xg1_fields() {
        let long = "Ż".repeat(200);
        let sgf = format!(
            "(;FF[4]GM[6]CA[UTF-8]PW[Łukasz Żółć]PB[{long}]MI[length:3][game:0][ws:0][bs:0]RU[Crawford]\n;W[31hefe])\n"
        );
        let xg = export(&sgf);
        let header = &records(&xg)[0];
        assert_eq!(
            &header[9..21],
            b"\x0b?ukasz ?\xf3??",
            "string[40] keeps Latin-1"
        );
        let reimported = convert_xg(&xg).unwrap();
        let parsed = crate::import::parse_sgf(&reimported.sgf).unwrap();
        assert_eq!(parsed.players[0], "Łukasz Żółć");
        assert_eq!(parsed.players[1], "Ż".repeat(128));
    }
}
