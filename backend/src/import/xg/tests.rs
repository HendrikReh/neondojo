use super::{
    analysis::{analysis_property, checker_analysis, cube_analysis},
    container::{MAX_UNCOMPRESSED, RECORD_BYTES, crc32, i32_at, main_records, u32_at},
    convert_xg, xg_beaver_rule, xg_to_sgf,
};
use crate::import::{ImportedPosition, parse_sgf, sgf};
use crate::model::{ImportWarning, ImportWarningKind};
use flate2::{Compression, write::ZlibEncoder};
use std::io::Write;

#[test]
fn conversion_preserves_native_analysis_and_xg_depth() {
    let sgf = xg_to_sgf(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
    let trees = sgf::parse_trees(&sgf).unwrap();
    let first = trees[0]
        .iter()
        .find(|n| n.contains_key("XGA"))
        .expect("XG analysis must survive conversion");
    let data: serde_json::Value = sgf::decode_retained_analysis(&first["XGA"][0]).unwrap();
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
    let sgf = xg_to_sgf(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
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
    let sgf = xg_to_sgf(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
    let trees = sgf::parse_trees(&sgf).unwrap();
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
    let mut records = main_records(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
    let last = records
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
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
    let mut records = main_records(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
    let last = records
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
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
    let bytes = main_records(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
    let mut record = bytes
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
        .find(|r| r[8] == 2 && i32_at(*r, 232).unwrap() >= 0 && i32_at(*r, 16).unwrap() == 0)
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
    let sample = include_bytes!("../../../tests/fixtures/WC2014.xg");
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
    let sample = include_bytes!("../../../tests/fixtures/WC2014.xg");
    let records = main_records(sample).unwrap();
    let mut checker = records
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
        .find(|r| r[8] == 3)
        .unwrap()
        .to_vec();
    set_u32(&mut checker, 120, 0);
    assert!(checker_analysis(&checker, 1).unwrap().is_none());
    let mut cube = records
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
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
    let sample = include_bytes!("../../../tests/fixtures/WC2014.xg");
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
    let mut records = main_records(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
    let end = records
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
        .position(|r| r[8] == 4)
        .unwrap();
    records.truncate((end + 1) * RECORD_BYTES);
    set_u32(&mut records, 552, 26);
    set_u32(&mut records, 92, 99999);
    records[101] = 1;
    let sgf = xg_to_sgf(&wrap(&records, None)).unwrap();
    let parsed = parse_sgf(&sgf).unwrap();
    let expected = records
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
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
    let sample = include_bytes!("../../../tests/fixtures/WC2014.xg");
    let expected = parse_sgf(&xg_to_sgf(sample).unwrap()).unwrap();
    let mut records = main_records(sample).unwrap();
    let end = records
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, r)| r[8] == 4)
        .nth(1)
        .unwrap()
        .0;
    records.truncate((end + 1) * RECORD_BYTES);
    let sgf = xg_to_sgf(&wrap(&records, None)).unwrap();
    let actual = parse_sgf(&sgf).unwrap();
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
    let sample = include_bytes!("../../../tests/fixtures/WC2014.xg");
    let expected = parse_sgf(&xg_to_sgf(sample).unwrap()).unwrap();
    let mut records = main_records(sample).unwrap();
    // Game five starts at 1-3. Retain its first played move after an
    // accepted double, the following no-double cube record, and a rolled
    // checker record that has not been played yet.
    let last_played = records
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
        .position(|r| r[8] == 3 && i32_at(r, 108).unwrap() != 0)
        .unwrap();
    let game_header = records[..last_played * RECORD_BYTES]
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
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
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
        .filter(|r| r[8] == 3 && r[2308] == 1)
        .count();
    let sgf = xg_to_sgf(&wrap(&records, None)).unwrap();
    let actual = parse_sgf(&sgf).unwrap();
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
    let mut records = main_records(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
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
    let mut records = main_records(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
    let end = records
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
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
    let original = main_records(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
    let end = original
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
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
    let original = main_records(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
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
    let sample = include_bytes!("../../../tests/fixtures/WC2014.xg");
    let original = main_records(sample).unwrap();
    let expected = parse_sgf(&xg_to_sgf(sample).unwrap()).unwrap();
    for doubles in [false, true] {
        let mut move_no = 0;
        let (index, _) = original
            .as_chunks::<RECORD_BYTES>()
            .0
            .iter()
            .enumerate()
            .find(|(_, record)| {
                if record[8] != 3 || record[2308] != 1 {
                    return false;
                }
                let p = &expected.positions[move_no];
                move_no += 1;
                let record = *record;
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
        let actual = parse_sgf(&sgf).unwrap();
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
    let mut records = main_records(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
    // Code 2 is XG's accepted illegal play; other codes remain errors.
    set_u32(&mut records, RECORD_BYTES * 3 + 2480, 1);
    let error = xg_to_sgf(&wrap(&records, None)).unwrap_err().to_string();
    assert!(error.contains("invalid"), "{error}");
    assert!(error.contains("game 1"), "{error}");
    assert!(error.contains("checker ply 1"), "{error}");
}

#[test]
fn damaged_archives_truncation_and_decompression_limits_are_rejected() {
    let sample = include_bytes!("../../../tests/fixtures/WC2014.xg");
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
    let records = main_records(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
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

fn played_positions(sgf: &str) -> Vec<ImportedPosition> {
    parse_sgf(sgf).unwrap().positions
}

fn assert_same_positions(actual: &[ImportedPosition], expected: &[&ImportedPosition]) {
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
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, r)| r[8] == 3 && r[2308] == 1)
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn xg_accepted_illegal_play_continues_from_its_recorded_position() {
    let sample = include_bytes!("../../../tests/fixtures/WC2014.xg");
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
            vec![ImportWarning::at(
                ImportWarningKind::IllegalPlayReplaced,
                expected[ordinal].game_no,
                Some(expected[ordinal].ply)
            )]
        );
    }
}

#[test]
fn recorded_positions_must_follow_from_the_replayed_plays() {
    let mut records = main_records(include_bytes!("../../../tests/fixtures/WC2014.xg")).unwrap();
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
    let sample = include_bytes!("../../../tests/fixtures/WC2014.xg");
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
            .as_chunks::<RECORD_BYTES>()
            .0
            .iter()
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
            vec![ImportWarning::at(
                ImportWarningKind::CustomStartPosition,
                game_no,
                None
            )]
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
    let sample = include_bytes!("../../../tests/fixtures/WC2014.xg");
    let expected_sgf = xg_to_sgf(sample).unwrap();
    let expected = played_positions(&expected_sgf);
    let original = main_records(sample).unwrap();
    let footer = original
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
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
    let trees = sgf::parse_trees(&converted.sgf).unwrap();
    let expected_trees = sgf::parse_trees(&expected_sgf).unwrap();
    assert!(expected_trees[0][0].contains_key("RE"));
    assert!(!trees[0][0].contains_key("RE"));
    assert_eq!(trees[1][0].get("RE"), expected_trees[1][0].get("RE"));
    assert_eq!(
        converted.warnings,
        vec![ImportWarning::at(
            ImportWarningKind::UnfinishedGame,
            1,
            None
        )]
    );
}
