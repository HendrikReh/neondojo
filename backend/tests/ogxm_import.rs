use bgtutor::import::{ogxm_to_sgf, parse_sgf};

#[test]
fn archive_match_agrees_with_independent_gnubg_replay() {
    let sgf = ogxm_to_sgf(include_bytes!("fixtures/hedgehog-cdf.ogxm")).unwrap();
    let actual = parse_sgf(&sgf).unwrap();
    let oracle = parse_sgf(include_str!("fixtures/hedgehog-cdf.gnubg.sgf")).unwrap();
    assert_eq!(actual.players, oracle.players);
    assert_eq!(actual.match_length, 9);
    assert_eq!(actual.games.len(), 5);
    assert!(actual.games[4].crawford);
    assert_eq!(actual.positions.len(), oracle.positions.len());
    assert!(actual.positions.iter().any(|p| p.notation == "Cannot move"));
    for (p, q) in actual.positions.iter().zip(&oracle.positions) {
        assert_eq!((p.game_no, p.ply, p.player), (q.game_no, q.ply, q.player));
        assert_eq!(p.board, q.board, "game {}, ply {}", p.game_no, p.ply);
        assert_eq!(p.position_id, q.position_id);
        assert_eq!(p.match_id, q.match_id);
    }
    let metadata = bgtutor::databases::source_metadata(&sgf);
    assert_eq!(metadata.tournament.as_deref(), Some("CdF Super Jackpot"));
    assert_eq!(metadata.played_on.unwrap().to_string(), "2026-10-24");
    assert_eq!(
        (metadata.white_score, metadata.black_score),
        (Some(9), Some(3))
    );
}

#[test]
fn corrupt_archive_score_and_unknown_critical_sections_are_rejected() {
    let original = include_bytes!("fixtures/hedgehog-cdf.ogxm");
    let mut bad_score = original.to_vec();
    let names_end = bad_score
        .windows("Guilaume Gallé".len())
        .position(|bytes| bytes == "Guilaume Gallé".as_bytes())
        .unwrap()
        + "Guilaume Gallé".len();
    // This real export has rules and final score as the next two fields.
    assert_eq!(&bad_score[names_end..names_end + 3], &[1, 9, 3]);
    bad_score[names_end + 1] = 8;
    let error = ogxm_to_sgf(&bad_score).unwrap_err();
    assert!(error.to_string().contains("final score mismatch"));

    let mut unknown = original.to_vec();
    unknown[16..20].copy_from_slice(b"TEST");
    let error = ogxm_to_sgf(&unknown).unwrap_err();
    assert!(error.to_string().contains("critical OGXM section"));
}

#[test]
fn analyzed_archive_match_preserves_overshoot_bearoff_candidates() {
    let sgf = ogxm_to_sgf(include_bytes!("fixtures/hedgehog-cdf-analyzed.ogxm")).unwrap();
    let imported = parse_sgf(&sgf).unwrap();
    assert_eq!(imported.positions.len(), 266);
    // Counts independently read with the HedgeHog reference codec.
    assert_eq!(sgf.matches("OGA[").count(), 266);
    assert_eq!(sgf.matches("OGC[").count(), 140);
}
