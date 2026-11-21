//! Decoder for the public XG2 RichGame/ZLIBArchive container and TSaveRec.
//! Format references and fixture provenance live in tests/fixtures/README.md.
use super::{match_context::MatchContext, sgf::escape_value};
use crate::model::{ImportWarning, ImportWarningKind};
use anyhow::{Context, Result, bail, ensure};
use chrono::{Days, NaiveDate};
use flate2::read::ZlibDecoder;
use std::io::Read;

/// Size of one TSaveRec record in the `temp.xg` match stream.
pub(super) const RECORD_BYTES: usize = 2560;
/// Size of the fixed TRichGameHeader before the thumbnail and archive.
pub(super) const RICH_HEADER_BYTES: usize = 8232;
/// Size of one TZLBFileRec entry in the archive registry.
pub(super) const REGISTRY_ENTRY_BYTES: usize = 532;
/// Size of the TZLBArchiveRec footer that ends the archive.
pub(super) const ARCHIVE_FOOTER_BYTES: usize = 36;
/// TSaveRec match header magic (`MagicNumber`, the bytes `DMLI`).
pub(super) const MATCH_MAGIC: u32 = 0x494c4d44;
const MAX_FILE: usize = 32 * 1024 * 1024;
const MAX_UNCOMPRESSED: usize = 64 * 1024 * 1024;

fn u32_at(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(p..p + 4)
            .context("XG: truncated integer")?
            .try_into()?,
    ))
}
fn i32_at(b: &[u8], p: usize) -> Result<i32> {
    Ok(u32_at(b, p)? as i32)
}
fn usize_at(b: &[u8], p: usize) -> Result<usize> {
    let n = i32_at(b, p)?;
    ensure!(n >= 0, "XG: negative size/offset");
    Ok(n as usize)
}
fn bool_at(b: &[u8], p: usize) -> Result<bool> {
    let n = *b.get(p).context("XG: truncated boolean")?;
    ensure!(n <= 1, "XG: invalid boolean at offset {p}");
    Ok(n != 0)
}
const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut n = i as u32;
        let mut j = 0;
        while j < 8 {
            n = if n & 1 != 0 {
                (n >> 1) ^ 0xedb88320
            } else {
                n >> 1
            };
            j += 1;
        }
        table[i] = n;
        i += 1;
    }
    table
}
pub(super) fn crc32(bytes: &[u8]) -> u32 {
    const TABLE: [u32; 256] = crc_table();
    !bytes.iter().fold(!0u32, |crc, b| {
        (crc >> 8) ^ TABLE[((crc ^ u32::from(*b)) & 255) as usize]
    })
}
fn inflate(bytes: &[u8], expected: usize) -> Result<Vec<u8>> {
    ensure!(
        expected <= MAX_UNCOMPRESSED,
        "XG: decompressed data exceeds 64 MiB limit"
    );
    let mut decoder = ZlibDecoder::new(bytes);
    let mut out = Vec::with_capacity(expected.min(1024 * 1024));
    (&mut decoder)
        .take(expected as u64 + 1)
        .read_to_end(&mut out)
        .context("XG: invalid zlib stream")?;
    ensure!(
        out.len() == expected,
        "XG: decompressed size does not match archive registry"
    );
    ensure!(
        decoder.total_in() as usize == bytes.len(),
        "XG: unused bytes after zlib stream"
    );
    Ok(out)
}
pub(super) fn main_records(input: &[u8]) -> Result<Vec<u8>> {
    ensure!(input.len() <= MAX_FILE, "XG file exceeds 32 MiB limit");
    ensure!(
        input.get(..4) == Some(b"RGMH"),
        "Not an XG RichGame file (missing RGMH signature)"
    );
    ensure!(
        u32_at(input, 4)? == 1,
        "Unsupported XG RichGame header version"
    );
    ensure!(
        u32_at(input, 8)? as usize == RICH_HEADER_BYTES,
        "Unsupported XG RichGame header size"
    );
    // XG writes a zero thumbnail offset; the payload always follows the fixed
    // header and thumbnail, as specified by XG_format.pas.
    let thumbnail = u32_at(input, 20)? as usize;
    let start = RICH_HEADER_BYTES
        .checked_add(thumbnail)
        .context("XG: thumbnail size overflow")?;
    let archive = input.get(start..).context("XG: truncated thumbnail")?;
    ensure!(
        archive.len() >= ARCHIVE_FOOTER_BYTES,
        "XG: missing archive footer"
    );
    let footer_at = archive.len() - ARCHIVE_FOOTER_BYTES;
    let footer = &archive[footer_at..];
    ensure!(
        u32_at(footer, 0)? == crc32(&archive[..footer_at]),
        "XG archive CRC mismatch; file may be damaged"
    );
    let count = usize_at(footer, 4)?;
    ensure!((1..=16).contains(&count), "XG: invalid archive entry count");
    ensure!(u32_at(footer, 8)? == 1, "Unsupported XG archive version");
    let registry_size = usize_at(footer, 12)?;
    let registry_at = usize_at(footer, 16)?;
    ensure!(
        registry_at.checked_add(registry_size) == Some(footer_at),
        "XG: invalid registry bounds"
    );
    let registry_bytes = archive
        .get(registry_at..footer_at)
        .context("XG: registry outside archive")?;
    let registry = if bool_at(footer, 20)? {
        inflate(registry_bytes, count * REGISTRY_ENTRY_BYTES)?
    } else {
        ensure!(
            registry_bytes.len() == count * REGISTRY_ENTRY_BYTES,
            "XG: invalid uncompressed registry length"
        );
        registry_bytes.to_vec()
    };
    let mut main = None;
    let mut ranges = Vec::new();
    for entry in registry.chunks_exact(REGISTRY_ENTRY_BYTES) {
        let name = std::str::from_utf8(&entry[1..=usize::from(entry[0])])
            .context("XG: invalid archive entry name")?
            .to_ascii_lowercase();
        let original_size = usize_at(entry, 512)?;
        let compressed_size = usize_at(entry, 516)?;
        let offset = usize_at(entry, 520)?;
        let end = offset
            .checked_add(compressed_size)
            .context("XG: entry size overflow")?;
        ensure!(end <= registry_at, "XG: entry lies outside archive payload");
        ensure!(
            original_size <= MAX_UNCOMPRESSED,
            "XG: entry exceeds decompression limit"
        );
        ensure!(entry[528] <= 1, "XG: unsupported compression status");
        ensure!(
            !ranges.iter().any(|&(a, b)| offset < b && a < end),
            "XG: overlapping archive entries"
        );
        ranges.push((offset, end));
        if name.ends_with(".xg") {
            ensure!(main.is_none(), "XG: multiple match streams");
            let bytes = &archive[offset..end];
            let decoded = if entry[528] == 0 {
                inflate(bytes, original_size)?
            } else {
                ensure!(
                    bytes.len() == original_size,
                    "XG: stored entry size mismatch"
                );
                bytes.to_vec()
            };
            ensure!(
                crc32(&decoded) == u32_at(entry, 524)?,
                "XG match stream CRC mismatch"
            );
            main = Some(decoded);
        }
    }
    main.context("XG archive contains no .xg match stream")
}
/// Read the declared rule independently of whether any Beaver was played.
/// The played-line import continues to reject unsupported Beaver actions.
pub(crate) fn xg_beaver_rule(input: &[u8]) -> Result<bool> {
    let records = main_records(input)?;
    let header = records
        .get(..RECORD_BYTES)
        .context("XG: truncated match header")?;
    ensure!(header[8] == 0, "XG: match header must be the first record");
    bool_at(header, 102)
}
fn header_text(
    record: &[u8],
    unicode_at: usize,
    ansi_at: usize,
    ansi_limit: usize,
) -> Result<String> {
    let words: Vec<u16> = record[unicode_at..unicode_at + 258]
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|n| *n != 0)
        .collect();
    if !words.is_empty() {
        return String::from_utf16(&words).context("XG: invalid Unicode header text");
    }
    let count = usize::from(record[ansi_at]);
    ensure!(count <= ansi_limit, "XG: invalid short header text length");
    // The ANSI fallback is byte-preserving Latin-1. Current XG2 normally uses
    // the UTF-16 fields, including for names outside the western code page.
    Ok(record[ansi_at + 1..ansi_at + 1 + count]
        .iter()
        .map(|b| char::from(*b))
        .collect())
}
pub(super) fn read_match_context(input: &[u8]) -> Result<MatchContext> {
    let records = main_records(input)?;
    let header = records
        .get(..RECORD_BYTES)
        .context("XG: truncated match header")?;
    ensure!(header[8] == 0, "XG: match header must be the first record");
    ensure!(
        u32_at(header, 556)? == MATCH_MAGIC,
        "XG: invalid match magic"
    );
    ensure!(
        (24..=40).contains(&i32_at(header, 552)?),
        "Unsupported XG save version"
    );
    Ok(match_context(header))
}

fn match_context(header: &[u8]) -> MatchContext {
    // The integral part of Delphi TDateTime counts calendar days since
    // 1899-12-30. Zero is the unset date and the fraction is local time.
    let date = f64_at(header, 128)
        .ok()
        .filter(|days| (1.0..2_958_466.0).contains(days))
        .and_then(|days| {
            NaiveDate::from_ymd_opt(1899, 12, 30)?.checked_add_days(Days::new(days as u64))
        });
    let event = header_text(header, 622, 136, 128).ok();
    let round = header_text(header, 1654, 417, 128).ok();
    let place = header_text(header, 1396, 283, 128).ok();
    MatchContext::new(date, event.as_deref(), round.as_deref(), place.as_deref())
}

/// XG positions use NeonDojo's absolute board layout: player 1 (White) is
/// positive, index 25 is White's bar and index 0 is Black's bar.
fn xg_board(record: &[u8], at: usize) -> [i32; 26] {
    std::array::from_fn(|i| i32::from(record[at + i] as i8))
}
/// CubeA/CubeB store log2(value) signed by the absolute owner.
fn cube_state(exponent: i32) -> Result<(i32, i32)> {
    ensure!((-15..=15).contains(&exponent), "XG: invalid cube exponent");
    let value = if exponent == 0 {
        1
    } else {
        1 << exponent.abs()
    };
    Ok((value, exponent.signum()))
}
/// An SGF setup node for a recorded XG position. The SGF replay validates the
/// checker counts; this only rejects checkers on the opponent's bar index.
fn setup_node(board: &[i32; 26], turn: Option<char>, cube: Option<(i32, i32)>) -> Result<String> {
    ensure!(
        board[0] <= 0 && board[25] >= 0,
        "XG: recorded position has a checker on the wrong bar"
    );
    let mut white = String::new();
    let mut black = String::new();
    for (point, &count) in board.iter().enumerate() {
        // AB[y] is Black's bar (index 0); AW[y] is White's bar (index 25).
        let label = if point == 0 || point == 25 {
            'y'
        } else {
            char::from(b'a' + point as u8 - 1)
        };
        let target = if count > 0 { &mut white } else { &mut black };
        for _ in 0..count.unsigned_abs() {
            target.push('[');
            target.push(label);
            target.push(']');
        }
    }
    let mut node = String::from("\n;AE[a:y]");
    if !white.is_empty() {
        node.push_str("AW");
        node.push_str(&white);
    }
    if !black.is_empty() {
        node.push_str("AB");
        node.push_str(&black);
    }
    if let Some((value, owner)) = cube {
        node.push_str(&format!(
            "CV[{value}]CO[{}]",
            match owner {
                1 => 'w',
                -1 => 'b',
                _ => 'c',
            }
        ));
    }
    if let Some(turn) = turn {
        node.push_str(&format!("PL[{turn}]"));
    }
    Ok(node)
}
fn color(active: i32) -> Result<char> {
    match active {
        1 => Ok('W'),
        -1 => Ok('B'),
        _ => bail!("XG: invalid active player {active}"),
    }
}
fn coordinate(point: i32, active: i32) -> Result<char> {
    match point {
        -6..=-1 => Ok('z'),
        24 => Ok('y'),
        0..=23 => Ok((if active == 1 {
            b'a' + point as u8
        } else {
            b'x' - point as u8
        }) as char),
        _ => bail!("XG: invalid checker coordinate {point}"),
    }
}

fn f32_at(b: &[u8], p: usize) -> Result<f64> {
    let value = f32::from_bits(u32_at(b, p)?) as f64;
    ensure!(value.is_finite(), "XG: non-finite evaluation");
    Ok(value)
}
fn f64_at(b: &[u8], p: usize) -> Result<f64> {
    let value = f64::from_le_bytes(
        b.get(p..p + 8)
            .context("XG: truncated evaluation")?
            .try_into()?,
    );
    ensure!(value.is_finite(), "XG: non-finite evaluation");
    Ok(value)
}
fn level(value: i32) -> (String, Option<i32>, &'static str) {
    match value {
        0..=6 => (format!("{}-ply", value + 1), Some(value + 1), "evaluation"),
        12 => ("3-ply red".into(), Some(3), "evaluation"),
        100 => ("Rollout".into(), None, "rollout"),
        998 => ("Opening Book V2".into(), None, "opening_book"),
        999 => ("Opening Book V1".into(), None, "opening_book"),
        1000 => ("XGRoller".into(), None, "xgroller"),
        1001 => ("XGRoller+".into(), None, "xgroller"),
        1002 => ("XGRoller++".into(), None, "xgroller"),
        _ => (format!("Unknown (XG level {value})"), None, "unknown"),
    }
}
fn native_notation(endpoints: &[i32]) -> String {
    let point = |n: i32| {
        if n == 24 {
            "bar".into()
        } else if n < 0 {
            "off".into()
        } else {
            (n + 1).to_string()
        }
    };
    let moves: Vec<_> = endpoints
        .chunks_exact(2)
        .take_while(|p| p[0] != -1)
        .map(|p| format!("{}/{}", point(p[0]), point(p[1])))
        .collect();
    if moves.is_empty() {
        "Cannot move".into()
    } else {
        moves.join(" ")
    }
}
fn checker_analysis(record: &[u8], index: usize) -> Result<Option<serde_json::Value>> {
    use serde_json::json;
    let count = i32_at(record, 120)?;
    ensure!(
        (0..=32).contains(&count),
        "XG: invalid analysis candidate count"
    );
    if count == 0 {
        return Ok(None);
    }
    let mut candidates = Vec::new();
    for i in 0..count as usize {
        let code = i16::from_le_bytes(record[1280 + i * 4..1282 + i * 4].try_into()?) as i32;
        let (depth, plies, method) = level(code);
        let endpoints: Vec<_> = record[1024 + i * 8..1032 + i * 8]
            .iter()
            .map(|b| i32::from(*b as i8))
            .collect();
        let values: Vec<_> = (0..7)
            .map(|j| f32_at(record, 1408 + i * 28 + j * 4))
            .collect::<Result<_>>()?;
        candidates.push(json!({"rank":i+1,"notation":native_notation(&endpoints),"equity":values[6],"equity_kind":"cubeful","probabilities":{"lose_backgammon":values[0],"lose_gammon":values[1],"lose":values[2],"win":values[3],"win_gammon":values[4],"win_backgammon":values[5]},"depth":depth,"depth_plies":plies,"method":method,"raw":{"level":code,"is_double_byte":record[1282+i*4],"endpoints":endpoints,"values":values,"rollout_index":i32_at(record,2344+i*4)?}}));
    }
    let first = &candidates[0];
    Ok(Some(
        json!({"engine":"eXtreme Gammon","engine_version":null,"depth":first["depth"],"depth_plies":first["depth_plies"],"method":first["method"],"candidates":candidates,"raw":{"record_index":index,"analyze_level":i32_at(record,2472)?,"requested_level":i32_at(record,160)?,"equity_loss":f64_at(record,2312)?,"candidate_count":count}}),
    ))
}
fn cube_analysis(record: &[u8], index: usize) -> Result<Option<serde_json::Value>> {
    use serde_json::json;
    let code = i32_at(record, 232)?;
    if code < 0 {
        return Ok(None);
    }
    let (depth, plies, method) = level(code);
    let no_double: Vec<_> = (0..7)
        .map(|j| f32_at(record, 124 + j * 4))
        .collect::<Result<_>>()?;
    let double_take: Vec<_> = (0..7)
        .map(|j| f32_at(record, 168 + j * 4))
        .collect::<Result<_>>()?;
    let mut value = json!({"engine":"eXtreme Gammon","engine_version":null,"depth":depth,"depth_plies":plies,"method":method,"candidates":[],"cube":{"equity_kind":"cubeful","perspective":"doubler","no_double":f32_at(record,152)?,"double_take":f32_at(record,156)?,"double_pass":f32_at(record,160)?},"raw":{"record_index":index,"action":i32_at(record,16)?,"analyze_level":code,"requested_level":i32_at(record,256)?,"equity_loss":f64_at(record,200)?,"take_equity_loss":f64_at(record,216)?,"rollout_index":i32_at(record,224)?,"no_double_values":no_double,"double_take_values":double_take}});
    let action = i32_at(record, 16)?;
    if action < 0 {
        value["played"] = json!(format!("Unknown (XG cube action {action})"));
    }
    Ok(Some(value))
}
fn analysis_property(out: &mut String, key: &str, value: &serde_json::Value) -> Result<()> {
    out.push_str(key);
    out.push('[');
    out.push_str(&super::sgf::encode_retained_analysis(value)?);
    out.push(']');
    Ok(())
}

/// A converted XG match and the source irregularities it resolved.
#[derive(Debug)]
pub struct XgImport {
    pub sgf: String,
    pub warnings: Vec<ImportWarning>,
}

/// Convert real binary XG2 files, retaining played moves, score and cube state.
/// Native analysis is retained independently through XGA/XGC SGF extensions.
pub fn xg_to_sgf(input: &[u8]) -> Result<String> {
    Ok(convert_xg(input)?.sgf)
}

/// Transcribed XG matches can start mid-game, contain plays that XG itself
/// marks as illegal, or start a game before the previous one was finished.
/// These become explicit SGF setups and import warnings. Every other recorded
/// position must match the replay of the preceding legal plays.
pub fn convert_xg(input: &[u8]) -> Result<XgImport> {
    let bytes = main_records(input)?;
    ensure!(
        bytes.len() >= RECORD_BYTES * 2 && bytes.len() % RECORD_BYTES == 0,
        "XG: invalid 2560-byte record layout"
    );
    let header = &bytes[..RECORD_BYTES];
    ensure!(header[8] == 0, "XG: match header must be the first record");
    ensure!(
        u32_at(header, 556)? == MATCH_MAGIC,
        "XG: invalid match magic"
    );
    let version = i32_at(header, 552)?;
    ensure!(
        (24..=40).contains(&version),
        "Unsupported XG save version {version}; supported XG2 versions are 24–40"
    );
    ensure!(
        i32_at(header, 96)? == 0,
        "Only standard backgammon XG matches are supported"
    );
    let raw_length = i32_at(header, 92)?;
    let length = if raw_length == 99999 { 0 } else { raw_length };
    ensure!(
        (0..=32767).contains(&length),
        "XG: match length outside GNUbg ID range"
    );
    let crawford_rule = bool_at(header, 100)?;
    let jacoby = bool_at(header, 101)?;
    let players = [
        header_text(header, 880, 9, 40)?,
        header_text(header, 1138, 50, 40)?,
    ];
    let context = match_context(header);
    let mut warnings = Vec::new();
    let mut out = String::new();
    let mut in_game = false;
    let mut game_in_progress = false;
    let mut saw_footer = false;
    let mut match_decided = false;
    let mut game_count = 0;
    let mut checker_no = 0;
    let mut cube = 1;
    let mut owner = 0; // XG player +/-1; zero is centered.
    let mut game_crawford = false;
    let mut game_scores = [0, 0];
    let mut root_end = 0;
    let mut board = super::sgf::initial_board();
    let mut pending_cube_analysis = None;
    let mut first_checker_record = None;
    for (index, record) in bytes.chunks_exact(RECORD_BYTES).enumerate().skip(1) {
        ensure!(!saw_footer, "XG: records after match footer");
        match record[8] {
            1 => {
                // An unfinished game followed by the next game was abandoned
                // in the source. Keep its moves without inventing a result.
                if in_game && game_in_progress {
                    out.push_str(")\n");
                    in_game = false;
                    warnings.push(ImportWarning {
                        kind: ImportWarningKind::UnfinishedGame,
                        game_no: game_count,
                        source_ply: None,
                    });
                }
                ensure!(!in_game, "XG: game header before previous game footer");
                // Cumulative money-session totals do not enter GNUbg equity
                // calculations; Match IDs use zero scores for money games.
                let white_score = if length == 0 { 0 } else { i32_at(record, 12)? };
                let black_score = if length == 0 { 0 } else { i32_at(record, 16)? };
                ensure!(
                    white_score >= 0 && black_score >= 0,
                    "XG: negative match score is not supported"
                );
                game_scores = [white_score, black_score];
                game_crawford = bool_at(record, 20)?;
                game_in_progress = bool_at(record, 52)?;
                // The first checker and cube records hold the game's actual
                // starting board and cube; transcripts can begin mid-game.
                let game_records = bytes[(index + 1) * RECORD_BYTES..]
                    .chunks_exact(RECORD_BYTES)
                    .enumerate()
                    .take_while(|(_, r)| !matches!(r[8], 1 | 4 | 5));
                let first_checker = game_records.clone().find(|(_, r)| r[8] == 3);
                first_checker_record = first_checker.map(|(offset, _)| index + 1 + offset);
                let start_board = first_checker
                    .map(|(_, r)| xg_board(r, 9))
                    .unwrap_or_else(super::sgf::initial_board);
                let start_cube = match game_records.map(|(_, r)| r).find(|r| matches!(r[8], 2 | 3))
                {
                    Some(r) => cube_state(i32_at(r, if r[8] == 2 { 32 } else { 108 })?)?,
                    None => (1, 0),
                };
                // XG keeps the opening setup here when the first checker record
                // was edited; any other stored board must be the actual start.
                let initial = xg_board(record, 21);
                ensure!(
                    initial.iter().all(|n| *n == 0)
                        || initial == super::sgf::initial_board()
                        || initial == start_board,
                    "XG: game {} header position does not match its first checker record",
                    game_count + 1
                );
                ensure!(
                    version < 26 || i32_at(record, 64)? == 0,
                    "XG automatic doubles are not supported"
                );
                let mut rules = Vec::new();
                if crawford_rule {
                    rules.push("Crawford");
                }
                if game_crawford {
                    rules.push("CrawfordGame");
                }
                if jacoby && length == 0 {
                    rules.push("Jacoby");
                }
                out.push_str(&format!("(;FF[4]GM[6]CA[UTF-8]AP[Neondojo:XG2]PW[{}]PB[{}]MI[length:{length}][game:{game_count}][ws:{white_score}][bs:{black_score}]",escape_value(&players[0]),escape_value(&players[1])));
                if game_count == 0 {
                    context.write_sgf(&mut out);
                }
                if !rules.is_empty() {
                    out.push_str(&format!("RU[{}]", rules.join(":")));
                }
                root_end = out.len();
                in_game = true;
                checker_no = 0;
                game_count += 1;
                board = start_board;
                (cube, owner) = start_cube;
                pending_cube_analysis = None;
                if board != super::sgf::initial_board() || start_cube != (1, 0) {
                    let turn = first_checker
                        .map(|(_, r)| i32_at(r, 64).and_then(color))
                        .transpose()?;
                    out.push_str(&setup_node(&board, turn, Some(start_cube))?);
                    warnings.push(ImportWarning {
                        kind: ImportWarningKind::CustomStartPosition,
                        game_no: game_count,
                        source_ply: None,
                    });
                }
            }
            2 => {
                ensure!(in_game, "XG: cube record outside game");
                ensure!(
                    !bool_at(record, 296)?,
                    "XG edited cube positions are not supported"
                );
                ensure!(
                    i32_at(record, 260)? == 0,
                    "XG contains a cube record marked invalid"
                );
                ensure!(
                    version < 27 || i32_at(record, 300)? == 0,
                    "XG automatic doubles are not supported"
                );
                let active = i32_at(record, 12)?;
                let action = i32_at(record, 16)?;
                ensure!((-2..=1).contains(&action), "XG: invalid double action");
                // XG can store an unavailable-cube placeholder before the opening
                // roll with no actor. It is not a played cube decision.
                if active == 0 && action == -2 {
                    continue;
                }
                let c = color(active)?;
                let analysis = cube_analysis(record, index)?;
                if action == 1 {
                    ensure!(!game_crawford, "XG: double in a Crawford game");
                    ensure!(
                        owner == 0 || owner == active,
                        "XG: double by player without cube ownership"
                    );
                    let take = i32_at(record, 20)?;
                    ensure!(
                        (0..=1).contains(&take),
                        "XG beavers/raccoons and unanswered doubles are not supported"
                    );
                    out.push_str(&format!("\n;{c}[double]"));
                    if let Some(ref value) = analysis {
                        analysis_property(&mut out, "XGC", value)?;
                    }
                    out.push_str(&format!(
                        ";{}[{}]",
                        color(-active)?,
                        if take == 1 { "take" } else { "drop" }
                    ));
                    if let Some(ref value) = analysis {
                        analysis_property(&mut out, "XGC", value)?;
                    }
                    if take == 1 {
                        cube *= 2;
                        ensure!(cube <= 32768, "XG: cube value exceeds GNUbg range");
                        owner = -active;
                    }
                } else if action == 0 || action == -1 {
                    pending_cube_analysis = analysis;
                }
            }
            3 => {
                ensure!(in_game, "XG: checker record outside game");
                // An edit of the first checker record defines the game's
                // starting setup, which is imported from its recorded board.
                ensure!(
                    !bool_at(record, 2528)? || first_checker_record == Some(index),
                    "XG edited checker positions are not supported (game {game_count}, record {index})"
                );
                let invalid = i32_at(record, 2480)?;
                // Code 2 is a played move that XG accepted although it is not
                // legal; XG keeps the position after it. Others stay rejected.
                ensure!(
                    invalid == 0 || invalid == 2,
                    "XG: checker record marked invalid (code {invalid}) in game {game_count}, checker ply {} (record {index})",
                    checker_no + 1
                );
                ensure!(
                    xg_board(record, 9) == board,
                    "XG: recorded position does not follow from the previous plays in game {game_count}, checker ply {} (record {index})",
                    checker_no + 1
                );
                ensure!(
                    version < 27 || i32_at(record, 2540)? == 0,
                    "XG automatic doubles are not supported"
                );
                if !bool_at(record, 2308)? {
                    continue;
                }
                checker_no += 1;
                let active = i32_at(record, 64)?;
                let c = color(active)?;
                // Unlike DataMoves.CubePos, CubeA uses absolute player1 /
                // player2 ownership. Confirmed against the official sample.
                ensure!(
                    cube_state(i32_at(record, 108)?)? == (cube, owner),
                    "XG: inconsistent cube context at record {index}"
                );
                if invalid == 2 {
                    // PositionEnd is from the mover's perspective. Continue
                    // from it; the illegal play is not a checker decision, so
                    // its native analysis and the preceding no-double
                    // analysis have no imported decision to attach to.
                    board = super::sgf::local_board(
                        &xg_board(record, 35),
                        if active == 1 { 0 } else { 1 },
                    );
                    out.push_str(&setup_node(&board, Some(color(-active)?), None)?);
                    pending_cube_analysis = None;
                    warnings.push(ImportWarning {
                        kind: ImportWarningKind::IllegalPlayReplaced,
                        game_no: game_count,
                        source_ply: Some(checker_no),
                    });
                    continue;
                }
                let d1 = i32_at(record, 100)?;
                let d2 = i32_at(record, 104)?;
                ensure!(
                    (1..=6).contains(&d1) && (1..=6).contains(&d2),
                    "XG: invalid dice"
                );
                let mut runs = Vec::new();
                let mut terminated = false;
                for pair in 0..4 {
                    let from = i32_at(record, 68 + pair * 8)?;
                    let to = i32_at(record, 72 + pair * 8)?;
                    if from == -1 {
                        terminated = true;
                        continue;
                    }
                    ensure!(!terminated, "XG: checker moves after terminator");
                    ensure!(from >= 0 && to < from, "XG: invalid checker move direction");
                    // Keep coordinate bounds strict before converting XG's
                    // zero-based player-relative points to the rule engine.
                    coordinate(from, active)?;
                    coordinate(to, active)?;
                    runs.push(((from + 1) as usize, (to + 1).max(0) as usize));
                }
                let steps = super::sgf::expand_xg_play(
                    &mut board,
                    if active == 1 { 0 } else { 1 },
                    [d1, d2],
                    &runs,
                )
                .with_context(|| {
                    format!(
                        "XG checker-play validation failed at record {index} (game {game_count})"
                    )
                })?;
                out.push_str(&format!("\n;{c}[{d1}{d2}"));
                for (from, to) in steps {
                    out.push(coordinate(from as i32 - 1, active)?);
                    out.push(coordinate(to as i32 - 1, active)?);
                }
                out.push(']');
                if let Some(value) = checker_analysis(record, index)? {
                    analysis_property(&mut out, "XGA", &value)?;
                }
                if let Some(mut value) = pending_cube_analysis.take() {
                    if value.get("played").is_none() {
                        value["played"] = serde_json::json!("No double");
                    }
                    analysis_property(&mut out, "XGC", &value)?;
                }
            }
            4 => {
                ensure!(in_game, "XG: game footer outside game");
                let final_scores = [i32_at(record, 12)?, i32_at(record, 16)?];
                match_decided = length > 0 && final_scores.iter().any(|s| *s >= length);
                // tsFooterGame contains the actual final scores. Pointswon can
                // hold producer sentinels for match resignations (e.g. 1998),
                // so derive points only from a consistent, one-sided delta.
                if length > 0 && !game_in_progress {
                    let side = match i32_at(record, 24)? {
                        1 => Some(0),
                        -1 => Some(1),
                        _ => None,
                    };
                    if let Some(side) = side
                        && final_scores[1 - side] == game_scores[1 - side]
                        && let Some(points) = final_scores[side].checked_sub(game_scores[side])
                        && (1..=32767).contains(&points)
                    {
                        let color = if side == 0 { 'W' } else { 'B' };
                        let termination = i32_at(record, 32)?;
                        let resignation = if (100..1000).contains(&termination) {
                            "R"
                        } else {
                            ""
                        };
                        out.insert_str(root_end, &format!("RE[{color}+{points}{resignation}]"));
                    }
                }
                out.push_str(")\n");
                in_game = false;
            }
            5 => {
                ensure!(!in_game, "XG: match footer before game footer");
                saw_footer = true;
            }
            kind => bail!("Unsupported XG record type {kind} at record {index}"),
        }
    }
    // XG can save between games, or during a game whose header explicitly
    // carries InProgress. Close only that declared unfinished game; a game
    // marked complete without its footer remains an incomplete record stream.
    if in_game && game_in_progress {
        out.push_str(")\n");
        in_game = false;
    }
    // A point match that reached its target still needs its match footer.
    // Ongoing matches and money sessions do not have a match winner/footer.
    // Archive CRCs and exact record bounds were checked before parsing records.
    ensure!(
        (saw_footer || !match_decided) && !in_game && game_count > 0,
        "XG: incomplete match record stream"
    );
    // Replay validates bars, hits, legal die distances, bearing off, and turns.
    super::parse_sgf(&out).context("XG checker-play validation failed")?;
    Ok(XgImport { sgf: out, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    #[test]
    fn conversion_preserves_native_analysis_and_xg_depth() {
        let sgf = xg_to_sgf(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        let trees = super::super::sgf::parse_trees(&sgf).unwrap();
        let first = trees[0]
            .iter()
            .find(|n| n.contains_key("XGA"))
            .expect("XG analysis must survive conversion");
        let data: serde_json::Value =
            super::super::sgf::decode_retained_analysis(&first["XGA"][0]).unwrap();
        assert_eq!(data["engine"], "eXtreme Gammon");
        assert_eq!(data["candidates"].as_array().unwrap().len(), 10);
        assert_eq!(data["candidates"][0]["depth"], "Opening Book V2");
        assert_eq!(data["candidates"][0]["raw"]["level"], 998);
        assert!(data["candidates"][0]["equity"].is_number());
        assert!(
            trees.iter().flatten().any(|n| n.contains_key("XGC")),
            "cube analysis must survive too"
        );
    }
    #[test]
    fn reads_match_context_from_the_published_world_championship_file() {
        let sgf = xg_to_sgf(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        let metadata = crate::databases::source_metadata(&sgf);
        assert_eq!(
            metadata.played_on,
            chrono::NaiveDate::from_ymd_opt(2014, 8, 10)
        );
        assert_eq!(
            metadata.tournament.as_deref(),
            Some("World Championship 2014")
        );
        assert_eq!(metadata.round.as_deref(), Some("Final - Tie Breaker"));
        assert_eq!(metadata.place.as_deref(), Some("Monte-Carlo"));
    }

    #[test]
    fn completed_match_preserves_final_footer_score_in_sgf() {
        let sgf = xg_to_sgf(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        let trees = super::super::sgf::parse_trees(&sgf).unwrap();
        assert_eq!(
            trees.last().unwrap()[0].get("RE"),
            Some(&vec!["W+2".to_owned()])
        );
        let metadata = crate::databases::source_metadata(&sgf);
        assert_eq!(
            (metadata.white_score, metadata.black_score),
            (Some(11), Some(9))
        );
        assert_eq!(metadata.result_state, "completed");
    }

    #[test]
    fn unknown_footer_cannot_supply_a_match_outcome() {
        let mut records = main_records(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        let last = records
            .chunks_exact(RECORD_BYTES)
            .rposition(|r| r[8] == 4)
            .unwrap()
            * RECORD_BYTES;
        set_u32(&mut records, last + 24, 0); // no recorded winner
        let sgf = xg_to_sgf(&wrap(&records, None)).unwrap();
        let metadata = crate::databases::source_metadata(&sgf);
        assert_eq!(
            (metadata.white_score, metadata.black_score),
            (Some(9), Some(9))
        );
        assert_eq!(metadata.result_state, "ongoing");
    }

    #[test]
    fn resignation_uses_recorded_score_delta_when_points_field_is_a_sentinel() {
        let mut records = main_records(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        let last = records
            .chunks_exact(RECORD_BYTES)
            .rposition(|r| r[8] == 4)
            .unwrap()
            * RECORD_BYTES;
        set_u32(&mut records, last + 28, 1998);
        set_u32(&mut records, last + 32, 104);
        let sgf = xg_to_sgf(&wrap(&records, None)).unwrap();
        let metadata = crate::databases::source_metadata(&sgf);
        assert_eq!(
            (metadata.white_score, metadata.black_score),
            (Some(11), Some(9))
        );
        assert_eq!(metadata.result_state, "completed");
    }

    #[test]
    fn native_payload_does_not_inflate_long_matches_past_sgf_limits() {
        let value = serde_json::json!({"raw":"precision retained ".repeat(30_000)});
        let mut out = String::new();
        analysis_property(&mut out, "XGA", &value).unwrap();
        assert!(
            out.len() < 10_000,
            "foreign analysis must use bounded compressed SGF transport"
        );
    }
    #[test]
    fn unknown_native_cube_actions_keep_their_source_label() {
        let bytes = main_records(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        let mut record = bytes
            .chunks_exact(RECORD_BYTES)
            .find(|r| r[8] == 2 && i32_at(r, 232).unwrap() >= 0 && i32_at(r, 16).unwrap() == 0)
            .unwrap()
            .to_vec();
        set_u32(&mut record, 16, u32::MAX);
        let data = cube_analysis(&record, 1).unwrap().unwrap();
        assert_eq!(data["raw"]["action"], -1);
        assert_eq!(data["played"], "Unknown (XG cube action -1)");
    }

    fn compress(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }
    fn set_u32(bytes: &mut [u8], offset: usize, n: u32) {
        bytes[offset..offset + 4].copy_from_slice(&n.to_le_bytes());
    }
    #[test]
    fn native_beaver_rule_reads_header_without_inventing_it_from_actions() {
        let sample = include_bytes!("../../tests/fixtures/WC2014.xg");
        assert!(
            !xg_beaver_rule(sample).unwrap(),
            "public fixture declares Beaver off"
        );
        let mut records = main_records(sample).unwrap();
        for expected in [false, true] {
            records[102] = u8::from(expected);
            assert_eq!(xg_beaver_rule(&wrap(&records, None)).unwrap(), expected);
        }
        records[102] = 2;
        assert!(xg_beaver_rule(&wrap(&records, None)).is_err());
        records[102] = 0;
        records[8] = 1;
        assert!(xg_beaver_rule(&wrap(&records, None)).is_err());
    }
    #[test]
    fn unanalysed_native_records_do_not_project_zero_equities() {
        let sample = include_bytes!("../../tests/fixtures/WC2014.xg");
        let records = main_records(sample).unwrap();
        let mut checker = records
            .chunks_exact(RECORD_BYTES)
            .find(|r| r[8] == 3)
            .unwrap()
            .to_vec();
        set_u32(&mut checker, 120, 0);
        assert!(checker_analysis(&checker, 1).unwrap().is_none());
        let mut cube = records
            .chunks_exact(RECORD_BYTES)
            .find(|r| r[8] == 2)
            .unwrap()
            .to_vec();
        set_u32(&mut cube, 232, u32::MAX);
        assert!(cube_analysis(&cube, 1).unwrap().is_none());
    }
    /// Write the documented binary container around actual fixture records.
    /// This lets malformed-record tests reach the semantic checks with valid
    /// compression and CRCs instead of failing at the outer checksum first.
    fn wrap(records: &[u8], declared_size: Option<u32>) -> Vec<u8> {
        let compressed = compress(records);
        let mut registry = vec![0u8; 532];
        registry[0] = 7;
        registry[1..8].copy_from_slice(b"temp.xg");
        set_u32(
            &mut registry,
            512,
            declared_size.unwrap_or(records.len() as u32),
        );
        set_u32(&mut registry, 516, compressed.len() as u32);
        set_u32(&mut registry, 524, crc32(records));
        registry[529] = 1;
        let registry = compress(&registry);
        let mut footer = vec![0u8; 36];
        set_u32(&mut footer, 4, 1);
        set_u32(&mut footer, 8, 1);
        set_u32(&mut footer, 12, registry.len() as u32);
        set_u32(&mut footer, 16, compressed.len() as u32);
        footer[20] = 1;
        let mut archive = compressed;
        archive.extend_from_slice(&registry);
        set_u32(&mut footer, 0, crc32(&archive));
        archive.extend_from_slice(&footer);
        let mut header = vec![0u8; 8232];
        header[..4].copy_from_slice(b"RGMH");
        set_u32(&mut header, 4, 1);
        set_u32(&mut header, 8, 8232);
        header.extend_from_slice(&archive);
        header
    }

    #[test]
    fn unavailable_cube_placeholder_can_have_no_active_player() {
        let sample = include_bytes!("../../tests/fixtures/WC2014.xg");
        let expected = xg_to_sgf(sample).unwrap();
        let mut records = main_records(sample).unwrap();
        let cube_record = RECORD_BYTES * 2;
        assert_eq!(records[cube_record + 8], 2);
        assert_eq!(i32_at(&records, cube_record + 16).unwrap(), -2);
        set_u32(&mut records, cube_record + 12, 0);
        let converted = xg_to_sgf(&wrap(&records, None)).unwrap();
        assert_eq!(converted, expected);
    }

    #[test]
    fn completed_money_games_can_end_without_a_match_footer() {
        let mut records = main_records(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        let end = records
            .chunks_exact(RECORD_BYTES)
            .position(|r| r[8] == 4)
            .unwrap();
        records.truncate((end + 1) * RECORD_BYTES);
        set_u32(&mut records, 552, 26);
        set_u32(&mut records, 92, 99999);
        records[101] = 1;
        let sgf = xg_to_sgf(&wrap(&records, None)).unwrap();
        let parsed = super::super::parse_sgf(&sgf).unwrap();
        let expected = records
            .chunks_exact(RECORD_BYTES)
            .filter(|r| r[8] == 3 && r[2308] == 1)
            .count();
        assert_eq!(parsed.match_length, 0);
        assert_eq!(parsed.positions.len(), expected);
        assert!(expected > 0);
        assert!(sgf.contains("Jacoby"));
        let metadata = crate::databases::source_metadata(&sgf);
        assert_eq!(metadata.result_state, "unknown");
        assert_eq!((metadata.white_score, metadata.black_score), (None, None));
        assert!(parsed.positions.iter().all(|p| p.game_no == 1));
        // Ending inside a game is still an incomplete stream.
        records.truncate(end * RECORD_BYTES);
        assert!(
            xg_to_sgf(&wrap(&records, None))
                .unwrap_err()
                .to_string()
                .contains("incomplete")
        );
    }

    #[test]
    fn point_match_saved_between_games_preserves_all_played_positions() {
        let sample = include_bytes!("../../tests/fixtures/WC2014.xg");
        let expected = super::super::parse_sgf(&xg_to_sgf(sample).unwrap()).unwrap();
        let mut records = main_records(sample).unwrap();
        let end = records
            .chunks_exact(RECORD_BYTES)
            .enumerate()
            .filter(|(_, r)| r[8] == 4)
            .nth(1)
            .unwrap()
            .0;
        records.truncate((end + 1) * RECORD_BYTES);
        let sgf = xg_to_sgf(&wrap(&records, None)).unwrap();
        let actual = super::super::parse_sgf(&sgf).unwrap();
        let expected: Vec<_> = expected
            .positions
            .iter()
            .filter(|p| p.game_no <= 2)
            .collect();
        assert_eq!(actual.match_length, 11);
        assert_eq!(actual.positions.len(), expected.len());
        for (a, b) in actual.positions.iter().zip(expected) {
            assert_eq!(a.position_id, b.position_id);
            assert_eq!(a.match_id, b.match_id);
            assert_eq!(a.board, b.board);
            assert_eq!((a.game_no, a.ply), (b.game_no, b.ply));
        }
    }

    #[test]
    fn in_progress_game_preserves_played_positions_and_nonzero_score_cube_context() {
        let sample = include_bytes!("../../tests/fixtures/WC2014.xg");
        let expected = super::super::parse_sgf(&xg_to_sgf(sample).unwrap()).unwrap();
        let mut records = main_records(sample).unwrap();
        // Game five starts at 1-3. Retain its first played move after an
        // accepted double, the following no-double cube record, and a rolled
        // checker record that has not been played yet.
        let last_played = records
            .chunks_exact(RECORD_BYTES)
            .position(|r| r[8] == 3 && i32_at(r, 108).unwrap() != 0)
            .unwrap();
        let game_header = records[..last_played * RECORD_BYTES]
            .chunks_exact(RECORD_BYTES)
            .rposition(|r| r[8] == 1)
            .unwrap();
        assert_eq!(
            i32_at(&records, game_header * RECORD_BYTES + 12).unwrap(),
            1
        );
        assert_eq!(
            i32_at(&records, game_header * RECORD_BYTES + 16).unwrap(),
            3
        );
        assert_eq!(records[(last_played + 1) * RECORD_BYTES + 8], 2);
        assert_eq!(records[(last_played + 2) * RECORD_BYTES + 8], 3);
        records.truncate((last_played + 3) * RECORD_BYTES);
        records[game_header * RECORD_BYTES + 52] = 1; // InProgress
        records[(last_played + 2) * RECORD_BYTES + 2308] = 0; // Played
        let played_count = records
            .chunks_exact(RECORD_BYTES)
            .filter(|r| r[8] == 3 && r[2308] == 1)
            .count();
        let sgf = xg_to_sgf(&wrap(&records, None)).unwrap();
        let actual = super::super::parse_sgf(&sgf).unwrap();
        assert_eq!(actual.positions.len(), played_count);
        assert_eq!(actual.positions.last().unwrap().game_no, 5);
        for (a, b) in actual.positions.iter().zip(&expected.positions) {
            assert_eq!(a.position_id, b.position_id);
            assert_eq!(a.match_id, b.match_id);
            assert_eq!(a.board, b.board);
            assert_eq!((a.game_no, a.ply), (b.game_no, b.ply));
        }
        // InProgress never bypasses unsupported-position checks, including
        // the final rolled but unplayed checker record.
        records[(last_played + 2) * RECORD_BYTES + 2528] = 1;
        assert!(
            xg_to_sgf(&wrap(&records, None))
                .unwrap_err()
                .to_string()
                .contains("edited")
        );
    }

    #[test]
    fn completed_point_match_without_its_footer_is_rejected() {
        let mut records = main_records(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        assert_eq!(records[records.len() - RECORD_BYTES + 8], 5);
        records.truncate(records.len() - RECORD_BYTES);
        assert!(
            xg_to_sgf(&wrap(&records, None))
                .unwrap_err()
                .to_string()
                .contains("incomplete")
        );
    }

    #[test]
    fn a_game_marked_complete_cannot_be_missing_its_game_footer() {
        let mut records = main_records(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        let end = records
            .chunks_exact(RECORD_BYTES)
            .position(|r| r[8] == 4)
            .unwrap();
        records.truncate(end * RECORD_BYTES);
        assert_eq!(records[RECORD_BYTES + 52], 0);
        assert!(
            xg_to_sgf(&wrap(&records, None))
                .unwrap_err()
                .to_string()
                .contains("incomplete")
        );
    }

    #[test]
    fn a_match_footer_cannot_replace_a_missing_game_footer() {
        let original = main_records(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        let end = original
            .chunks_exact(RECORD_BYTES)
            .position(|r| r[8] == 4)
            .unwrap();
        let mut records = original[..end * RECORD_BYTES].to_vec();
        records.extend_from_slice(&original[original.len() - RECORD_BYTES..]);
        assert!(
            xg_to_sgf(&wrap(&records, None))
                .unwrap_err()
                .to_string()
                .contains("game footer")
        );
    }

    #[test]
    fn actual_cube_decisions_still_require_an_active_player() {
        let original = main_records(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        for action in [0, 1] {
            let mut records = original.clone();
            let cube_record = RECORD_BYTES * 2;
            set_u32(&mut records, cube_record + 12, 0);
            set_u32(&mut records, cube_record + 16, action);
            assert!(
                xg_to_sgf(&wrap(&records, None))
                    .unwrap_err()
                    .to_string()
                    .contains("invalid active player 0")
            );
        }
    }

    #[test]
    fn compact_checker_runs_expand_to_the_original_positions() {
        let sample = include_bytes!("../../tests/fixtures/WC2014.xg");
        let original = main_records(sample).unwrap();
        let expected = super::super::parse_sgf(&xg_to_sgf(sample).unwrap()).unwrap();
        for doubles in [false, true] {
            let mut move_no = 0;
            let (index, _) = original
                .chunks_exact(RECORD_BYTES)
                .enumerate()
                .find(|(_, record)| {
                    if record[8] != 3 || record[2308] != 1 {
                        return false;
                    }
                    let p = &expected.positions[move_no];
                    move_no += 1;
                    let middle = i32_at(record, 72).unwrap();
                    let absolute = if p.player == 0 {
                        middle + 1
                    } else {
                        24 - middle
                    };
                    middle >= 0
                        && middle == i32_at(record, 76).unwrap()
                        && (i32_at(record, 100).unwrap() == i32_at(record, 104).unwrap()) == doubles
                        && p.board[absolute as usize] * if p.player == 0 { 1 } else { -1 } >= 0
                })
                .expect("fixture has a joinable non-hitting checker run");
            let mut records = original.clone();
            let start = index * RECORD_BYTES;
            let end = u32_at(&records, start + 80).unwrap();
            set_u32(&mut records, start + 72, end);
            records.copy_within(start + 84..start + 100, start + 76);
            set_u32(&mut records, start + 92, u32::MAX);
            set_u32(&mut records, start + 96, u32::MAX);
            let sgf = xg_to_sgf(&wrap(&records, None)).unwrap();
            let actual = super::super::parse_sgf(&sgf).unwrap();
            assert_eq!(actual.positions.len(), expected.positions.len());
            for (a, b) in actual.positions.iter().zip(&expected.positions) {
                assert_eq!(a.position_id, b.position_id);
                assert_eq!(a.match_id, b.match_id);
                assert_eq!(a.board, b.board);
            }
        }
    }

    #[test]
    fn invalid_marked_checker_reports_its_game_and_ply() {
        let mut records = main_records(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        // Code 2 is XG's accepted illegal play; other codes remain errors.
        set_u32(&mut records, RECORD_BYTES * 3 + 2480, 1);
        let error = xg_to_sgf(&wrap(&records, None)).unwrap_err().to_string();
        assert!(error.contains("invalid"), "{error}");
        assert!(error.contains("game 1"), "{error}");
        assert!(error.contains("checker ply 1"), "{error}");
    }

    #[test]
    fn damaged_archives_truncation_and_decompression_limits_are_rejected() {
        let sample = include_bytes!("../../tests/fixtures/WC2014.xg");
        for end in [0, 3, 4, 8, 20, 8231, 8232, 17502, sample.len() - 1] {
            assert!(xg_to_sgf(&sample[..end]).is_err());
        }
        let mut corrupt = sample.to_vec();
        corrupt[18000] ^= 1;
        assert!(xg_to_sgf(&corrupt).unwrap_err().to_string().contains("CRC"));
        let too_large = wrap(b"small", Some(MAX_UNCOMPRESSED as u32 + 1));
        assert!(
            xg_to_sgf(&too_large)
                .unwrap_err()
                .to_string()
                .contains("limit")
        );
        let incorrect_size = wrap(b"small", Some(10));
        assert!(
            xg_to_sgf(&incorrect_size)
                .unwrap_err()
                .to_string()
                .contains("size")
        );
    }

    #[test]
    fn unsupported_record_versions_variants_and_layouts_are_rejected() {
        let records = main_records(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        for version in [0u32, 23, 41, 999] {
            let mut changed = records.clone();
            set_u32(&mut changed, 552, version);
            assert!(
                xg_to_sgf(&wrap(&changed, None))
                    .unwrap_err()
                    .to_string()
                    .contains("version")
            );
        }
        let mut variant = records.clone();
        set_u32(&mut variant, 96, 1);
        assert!(
            xg_to_sgf(&wrap(&variant, None))
                .unwrap_err()
                .to_string()
                .contains("standard")
        );
        let mut edited = records.clone();
        // The first checker record may define an edited start; later ones not.
        assert_eq!(edited[RECORD_BYTES * 5 + 8], 3);
        edited[RECORD_BYTES * 5 + 2528] = 1;
        assert!(
            xg_to_sgf(&wrap(&edited, None))
                .unwrap_err()
                .to_string()
                .contains("edited")
        );
        assert!(
            xg_to_sgf(&wrap(&records[..records.len() - 1], None))
                .unwrap_err()
                .to_string()
                .contains("layout")
        );
    }

    fn played_positions(sgf: &str) -> Vec<crate::import::ImportedPosition> {
        super::super::parse_sgf(sgf).unwrap().positions
    }
    fn assert_same_positions(
        actual: &[crate::import::ImportedPosition],
        expected: &[&crate::import::ImportedPosition],
    ) {
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(expected) {
            assert_eq!(a.board, b.board);
            assert_eq!(a.position_id, b.position_id);
            assert_eq!(a.match_id, b.match_id);
            assert_eq!(a.game_no, b.game_no);
        }
    }
    /// Index of every played checker record, in the order of parsed positions.
    fn played_records(records: &[u8]) -> Vec<usize> {
        records
            .chunks_exact(RECORD_BYTES)
            .enumerate()
            .filter(|(_, r)| r[8] == 3 && r[2308] == 1)
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn xg_accepted_illegal_play_continues_from_its_recorded_position() {
        let sample = include_bytes!("../../tests/fixtures/WC2014.xg");
        let expected = played_positions(&xg_to_sgf(sample).unwrap());
        let original = main_records(sample).unwrap();
        let played = played_records(&original);
        // One White and one Black play, so both PositionEnd perspectives are used.
        for ordinal in [4, 5] {
            let index = played[ordinal];
            let start = index * RECORD_BYTES;
            let mut records = original.clone();
            set_u32(&mut records, start + 2480, 2);
            // XG keeps whatever was entered; the moves themselves are ignored.
            for offset in (68..100).step_by(4) {
                set_u32(&mut records, start + offset, 3);
            }
            let converted = convert_xg(&wrap(&records, None)).unwrap();
            let actual = played_positions(&converted.sgf);
            let kept: Vec<_> = expected
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != ordinal)
                .map(|(_, p)| p)
                .collect();
            assert_same_positions(&actual, &kept);
            assert_eq!(
                converted.warnings,
                vec![ImportWarning {
                    kind: ImportWarningKind::IllegalPlayReplaced,
                    game_no: expected[ordinal].game_no,
                    source_ply: Some(expected[ordinal].ply),
                }]
            );
        }
    }

    #[test]
    fn recorded_positions_must_follow_from_the_replayed_plays() {
        let mut records = main_records(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        let index = played_records(&records)[6];
        // Move a White checker from the 6-point to the 5-point before the play.
        let start = index * RECORD_BYTES + 9;
        records[start + 6] = (records[start + 6] as i8 - 1) as u8;
        records[start + 5] = (records[start + 5] as i8 + 1) as u8;
        let error = xg_to_sgf(&wrap(&records, None)).unwrap_err().to_string();
        assert!(error.contains("does not follow"), "{error}");
        assert!(error.contains("game 1, checker ply 7"), "{error}");
    }

    #[test]
    fn transcripts_can_start_a_game_from_a_recorded_position_and_cube() {
        let sample = include_bytes!("../../tests/fixtures/WC2014.xg");
        let expected = played_positions(&xg_to_sgf(sample).unwrap());
        let original = main_records(sample).unwrap();
        let played = played_records(&original);
        for with_cube in [false, true] {
            // Start a game at a played checker record; with_cube picks one after
            // a take, so the start cube must come from the record's cube state.
            let ordinal = (0..played.len())
                .find(|&n| {
                    let cube = i32_at(&original, played[n] * RECORD_BYTES + 108).unwrap();
                    (cube != 0) == with_cube && expected[n].ply > 3
                })
                .unwrap();
            let first = played[ordinal];
            let header = original[..first * RECORD_BYTES]
                .chunks_exact(RECORD_BYTES)
                .rposition(|r| r[8] == 1)
                .unwrap();
            let game_no = expected[ordinal].game_no;
            let mut records = original[..(header + 1) * RECORD_BYTES].to_vec();
            records.extend_from_slice(&original[first * RECORD_BYTES..]);
            // XG flags a hand-entered starting position as edited.
            records[(header + 1) * RECORD_BYTES + 2528] = 1;
            let converted = convert_xg(&wrap(&records, None)).unwrap();
            let actual = played_positions(&converted.sgf);
            let kept: Vec<_> = expected
                .iter()
                .enumerate()
                .filter(|(i, p)| p.game_no != game_no || *i >= ordinal)
                .map(|(_, p)| p)
                .collect();
            assert_same_positions(&actual, &kept);
            assert_eq!(actual.iter().find(|p| p.game_no == game_no).unwrap().ply, 1);
            assert_eq!(
                converted.warnings,
                vec![ImportWarning {
                    kind: ImportWarningKind::CustomStartPosition,
                    game_no,
                    source_ply: None,
                }]
            );
            // A standard header still has to agree with the stored start.
            let mut contradicting = records.clone();
            let earlier = played[ordinal - 1] * RECORD_BYTES;
            contradicting[header * RECORD_BYTES + 21..header * RECORD_BYTES + 47]
                .copy_from_slice(&original[earlier + 9..earlier + 35]);
            assert!(
                xg_to_sgf(&wrap(&contradicting, None))
                    .unwrap_err()
                    .to_string()
                    .contains("header position")
            );
        }
        // Edits after the first checker record remain unsupported.
        let mut edited = original.clone();
        edited[played[3] * RECORD_BYTES + 2528] = 1;
        assert!(
            xg_to_sgf(&wrap(&edited, None))
                .unwrap_err()
                .to_string()
                .contains("edited")
        );
    }

    #[test]
    fn an_unfinished_game_followed_by_the_next_game_keeps_its_moves_without_a_result() {
        let sample = include_bytes!("../../tests/fixtures/WC2014.xg");
        let expected_sgf = xg_to_sgf(sample).unwrap();
        let expected = played_positions(&expected_sgf);
        let original = main_records(sample).unwrap();
        let footer = original
            .chunks_exact(RECORD_BYTES)
            .position(|r| r[8] == 4)
            .unwrap();
        let mut records = original[..footer * RECORD_BYTES].to_vec();
        records.extend_from_slice(&original[(footer + 1) * RECORD_BYTES..]);
        // Without InProgress the missing footer stays a damaged record stream.
        assert!(
            xg_to_sgf(&wrap(&records, None))
                .unwrap_err()
                .to_string()
                .contains("before previous game footer")
        );
        records[RECORD_BYTES + 52] = 1;
        let converted = convert_xg(&wrap(&records, None)).unwrap();
        let actual = played_positions(&converted.sgf);
        assert_same_positions(&actual, &expected.iter().collect::<Vec<_>>());
        let trees = super::super::sgf::parse_trees(&converted.sgf).unwrap();
        let expected_trees = super::super::sgf::parse_trees(&expected_sgf).unwrap();
        assert!(expected_trees[0][0].contains_key("RE"));
        assert!(!trees[0][0].contains_key("RE"));
        assert_eq!(trees[1][0].get("RE"), expected_trees[1][0].get("RE"));
        assert_eq!(
            converted.warnings,
            vec![ImportWarning {
                kind: ImportWarningKind::UnfinishedGame,
                game_no: 1,
                source_ply: None,
            }]
        );
    }
}
