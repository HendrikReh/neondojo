//! Decoder for the public XG2 RichGame/ZLIBArchive container and TSaveRec.
//! Format references and fixture provenance live in tests/fixtures/README.md.
use anyhow::{Context, Result, bail, ensure};
use flate2::read::ZlibDecoder;
use std::io::Read;

const RECORD_BYTES: usize = 2560;
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
fn crc32(bytes: &[u8]) -> u32 {
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
fn main_records(input: &[u8]) -> Result<Vec<u8>> {
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
        u32_at(input, 8)? == 8232,
        "Unsupported XG RichGame header size"
    );
    // XG writes a zero thumbnail offset; the payload always follows the fixed
    // header and thumbnail, as specified by XG_format.pas.
    let thumbnail = u32_at(input, 20)? as usize;
    let start = 8232usize
        .checked_add(thumbnail)
        .context("XG: thumbnail size overflow")?;
    let archive = input.get(start..).context("XG: truncated thumbnail")?;
    ensure!(archive.len() >= 36, "XG: missing archive footer");
    let footer_at = archive.len() - 36;
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
        inflate(registry_bytes, count * 532)?
    } else {
        ensure!(
            registry_bytes.len() == count * 532,
            "XG: invalid uncompressed registry length"
        );
        registry_bytes.to_vec()
    };
    let mut main = None;
    let mut ranges = Vec::new();
    for entry in registry.chunks_exact(532) {
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
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(']', "\\]")
}
fn name(record: &[u8], unicode_at: usize, ansi_at: usize) -> Result<String> {
    let words: Vec<u16> = record[unicode_at..unicode_at + 258]
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|n| *n != 0)
        .collect();
    if !words.is_empty() {
        return String::from_utf16(&words).context("XG: invalid Unicode player name");
    }
    let count = usize::from(record[ansi_at]);
    ensure!(count <= 40, "XG: invalid short player name length");
    // The ANSI fallback is byte-preserving Latin-1. Current XG2 normally uses
    // the UTF-16 fields, including for names outside the western code page.
    Ok(record[ansi_at + 1..ansi_at + 1 + count]
        .iter()
        .map(|b| char::from(*b))
        .collect())
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

/// Convert real binary XG2 files, retaining played moves, score and cube state.
/// Stored analysis, rollouts and RTF comments are deliberately not imported.
pub fn xg_to_sgf(input: &[u8]) -> Result<String> {
    let bytes = main_records(input)?;
    ensure!(
        bytes.len() >= RECORD_BYTES * 2 && bytes.len() % RECORD_BYTES == 0,
        "XG: invalid 2560-byte record layout"
    );
    let header = &bytes[..RECORD_BYTES];
    ensure!(header[8] == 0, "XG: match header must be the first record");
    ensure!(
        u32_at(header, 556)? == 0x494c4d44,
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
    let players = [name(header, 880, 9)?, name(header, 1138, 50)?];
    let mut out = String::new();
    let mut in_game = false;
    let mut saw_footer = false;
    let mut game_count = 0;
    let mut checker_no = 0;
    let mut cube = 1;
    let mut owner = 0; // XG player +/-1; zero is centered.
    let mut game_crawford = false;
    let mut board = super::sgf::initial_board();
    for (index, record) in bytes.chunks_exact(RECORD_BYTES).enumerate().skip(1) {
        ensure!(!saw_footer, "XG: records after match footer");
        match record[8] {
            1 => {
                ensure!(!in_game, "XG: game header before previous game footer");
                // Cumulative money-session totals do not enter GNUbg equity
                // calculations; Match IDs use zero scores for money games.
                let white_score = if length == 0 { 0 } else { i32_at(record, 12)? };
                let black_score = if length == 0 { 0 } else { i32_at(record, 16)? };
                ensure!(
                    white_score >= 0 && black_score >= 0,
                    "XG: negative match score is not supported"
                );
                game_crawford = bool_at(record, 20)?;
                let initial: Vec<i32> =
                    record[21..47].iter().map(|b| i32::from(*b as i8)).collect();
                ensure!(
                    initial.iter().all(|n| *n == 0) || initial == super::sgf::initial_board(),
                    "XG games with custom starting positions are not supported"
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
                out.push_str(&format!("(;FF[4]GM[6]CA[UTF-8]AP[BGTutor:XG2]PW[{}]PB[{}]MI[length:{length}][game:{game_count}][ws:{white_score}][bs:{black_score}]",escape(&players[0]),escape(&players[1])));
                if !rules.is_empty() {
                    out.push_str(&format!("RU[{}]", rules.join(":")));
                }
                in_game = true;
                board = super::sgf::initial_board();
                checker_no = 0;
                game_count += 1;
                cube = 1;
                owner = 0;
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
                    out.push_str(&format!(
                        "\n;{c}[double];{}[{}]",
                        color(-active)?,
                        if take == 1 { "take" } else { "drop" }
                    ));
                    if take == 1 {
                        cube *= 2;
                        ensure!(cube <= 32768, "XG: cube value exceeds GNUbg range");
                        owner = -active;
                    }
                }
            }
            3 => {
                ensure!(in_game, "XG: checker record outside game");
                ensure!(
                    !bool_at(record, 2528)?,
                    "XG edited checker positions are not supported"
                );
                let invalid = i32_at(record, 2480)?;
                ensure!(
                    invalid == 0,
                    "XG: checker record marked invalid (code {invalid}) in game {game_count}, checker ply {} (record {index})",
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
                let cube_state = i32_at(record, 108)?;
                ensure!(
                    (-15..=15).contains(&cube_state),
                    "XG: invalid cube exponent"
                );
                let record_cube = if cube_state == 0 {
                    1
                } else {
                    1 << cube_state.abs()
                };
                // Unlike DataMoves.CubePos, CubeA uses absolute player1 /
                // player2 ownership. Confirmed against the official sample.
                let record_owner = cube_state.signum();
                ensure!(
                    record_cube == cube && record_owner == owner,
                    "XG: inconsistent cube context at record {index}"
                );
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
            }
            4 => {
                ensure!(in_game, "XG: game footer outside game");
                // Scores for the next game are authoritative in its header.
                out.push_str(")\n");
                in_game = false;
            }
            5 => {
                if in_game {
                    out.push_str(")\n");
                    in_game = false;
                }
                saw_footer = true;
            }
            kind => bail!("Unsupported XG record type {kind} at record {index}"),
        }
    }
    // Ongoing money sessions have no match winner/footer. Their final game
    // must still be closed; archive CRCs and exact record bounds were checked above.
    ensure!(
        (saw_footer || length == 0) && !in_game && game_count > 0,
        "XG: incomplete match record stream"
    );
    // Replay validates bars, hits, legal die distances, bearing off, and turns.
    super::parse_sgf(&out).context("XG checker-play validation failed")?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    fn compress(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }
    fn set_u32(bytes: &mut [u8], offset: usize, n: u32) {
        bytes[offset..offset + 4].copy_from_slice(&n.to_le_bytes());
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
        // A point match still requires its match footer.
        assert!(
            xg_to_sgf(&wrap(&records, None))
                .unwrap_err()
                .to_string()
                .contains("incomplete")
        );
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
        set_u32(&mut records, RECORD_BYTES * 3 + 2480, 2);
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
        edited[RECORD_BYTES * 3 + 2528] = 1;
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
}
