use super::*;

#[test]
fn opening_moves_preserve_absolute_board_and_escaped_names() {
    let m =
        parse_sgf(r"(;GM[6]FF[4]PW[A\]B]PB[Black]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])")
            .unwrap();
    assert_eq!(m.players[0], "A]B");
    assert_eq!(m.positions.len(), 2);
    assert_eq!(m.positions[0].position_id, "4HPwATDgc/ABMA");
    assert_eq!(m.positions[0].notation, "8/5 6/5");
    assert_eq!(m.positions[1].board[5], 2);
    assert_eq!(m.positions[1].board[8], 2);
    assert_eq!(m.positions[1].player, 1);
    // Independently verified with GNUbg 1.05.002 load/previous/set dice/show board.
    assert_eq!(m.positions[1].position_id, "sGfwATDgc/ABMA");
    assert_eq!(m.positions[1].match_id, "cAnqAAAAAAAE");
}

#[test]
fn official_xg_matches_gnubg_mat_conversion() {
    let xg = include_bytes!("../../tests/fixtures/WC2014.xg");
    let from_xg = parse_sgf(&xg_to_sgf(xg).unwrap()).unwrap();
    let from_mat = parse_sgf(include_str!("../../tests/fixtures/WC2014.gnubg.sgf")).unwrap();
    assert_eq!(from_xg.match_length, 11);
    assert_eq!(from_xg.positions.len(), 460);
    assert_eq!(from_mat.positions.len(), 460);
    // The published MAT exports the XG players in opposite color order.
    assert_eq!(from_xg.players[0], "Akiko Yazawa");
    assert_eq!(from_xg.players[1], "Douglas Mayfield");
    for (x, m) in from_xg.positions.iter().zip(&from_mat.positions) {
        assert_eq!(x.game_no, m.game_no);
        assert_eq!(x.ply, m.ply);
        assert_eq!(
            x.position_id, m.position_id,
            "game {} ply {}",
            x.game_no, x.ply
        );
        let mut xn = x.notation.split_whitespace().collect::<Vec<_>>();
        xn.sort();
        let mut mn = m.notation.split_whitespace().collect::<Vec<_>>();
        mn.sort();
        assert_eq!(xn, mn, "game {} ply {}", x.game_no, x.ply);
        assert_eq!(x.dice, m.dice);
        assert_eq!(
            from_xg.players[x.player as usize],
            from_mat.players[m.player as usize]
        );
        for (offset, width) in [(0, 4), (7, 1), (15, 6), (21, 15), (66, 1)] {
            assert_eq!(
                match_bits(&x.match_id, offset, width),
                match_bits(&m.match_id, offset, width)
            );
        }
        assert_eq!(
            match_bits(&x.match_id, 36, 15),
            match_bits(&m.match_id, 51, 15)
        );
        assert_eq!(
            match_bits(&x.match_id, 51, 15),
            match_bits(&m.match_id, 36, 15)
        );
        let xo = match_bits(&x.match_id, 4, 2);
        let mo = match_bits(&m.match_id, 4, 2);
        assert!(xo == 3 && mo == 3 || xo + mo == 1);
    }
    assert_eq!(from_mat.positions[7].position_id, "Y54pATDg28EBYA");
    assert_eq!(from_mat.positions[7].match_id, "cAlnAQAAAAAE");
    assert_eq!(from_mat.positions[7].notation, "Cannot move");
}

fn match_bits(id: &str, offset: usize, width: usize) -> i32 {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(id)
        .unwrap();
    (0..width)
        .map(|i| i32::from((bytes[(offset + i) / 8] >> ((offset + i) % 8)) & 1) << i)
        .sum()
}

#[test]
fn scores_cube_ownership_crawford_and_jacoby_are_encoded() {
    let m = parse_sgf("(;GM[6]MI[length:9][ws:2][bs:4]CV[2]CO[w];B[52lqqs])").unwrap();
    assert_eq!(m.positions[0].match_id, "QYkqASAAIAAE");
    let m =
        parse_sgf("(;GM[6]MI[length:7][ws:6][bs:2]RU[Crawford:CrawfordGame];W[31hefe])").unwrap();
    assert_eq!(match_bits(&m.positions[0].match_id, 7, 1), 1);
    assert_eq!(match_bits(&m.positions[0].match_id, 36, 15), 6);
    assert_eq!(match_bits(&m.positions[0].match_id, 51, 15), 2);
    assert!(
        parse_sgf(
            "(;GM[6]MI[length:7][ws:6]RU[Crawford:CrawfordGame];W[double];B[take];W[31hefe])"
        )
        .is_err()
    );
    let m = parse_sgf("(;GM[6]RU[Jacoby];W[double];B[take];W[31hefe];B[42qusu])").unwrap();
    assert_eq!(match_bits(&m.positions[0].match_id, 0, 4), 1);
    assert_eq!(match_bits(&m.positions[0].match_id, 4, 2), 1);
    assert_eq!(match_bits(&m.positions[0].match_id, 66, 1), 0);
}

#[test]
fn setup_bar_hits_bearing_off_and_multigame_reset() {
    let m = parse_sgf("(;GM[6]AE[a:y]AW[y]AB[x];W[11yxxwwvvu];B[11yaabbccd])").unwrap();
    assert_eq!(m.positions[0].notation, "bar/24* 24/23 23/22 22/21");
    assert_eq!(m.positions[1].board[0], -1);
    assert_eq!(m.positions[1].board[21], 1);
    assert_eq!(m.positions[1].board.iter().sum::<i32>(), 0);
    let m = parse_sgf("(;GM[6]AE[a:y]AW[f][f]AB[x];W[66fzfz])").unwrap();
    assert_eq!(m.positions[0].notation, "6/off 6/off");
    let m = parse_sgf("(;GM[6]MI[length:7]RE[W+2];W[31hefe])(;GM[6]MI[length:7];B[31qts t])");
    assert!(m.is_err());
    let m =
        parse_sgf("(;GM[6]MI[length:7]RE[W+2];W[31hefe])(;GM[6]MI[length:7];B[31qtst])").unwrap();
    assert_eq!(m.positions[1].game_no, 2);
    assert_eq!(m.positions[1].ply, 1);
    assert_eq!(match_bits(&m.positions[1].match_id, 36, 15), 2);
    assert_eq!(m.positions[1].position_id, "4HPwATDgc/ABMA");
}

#[test]
fn escaping_truncation_variations_and_illegal_plays_are_not_silently_accepted() {
    let m = parse_sgf("(;GM[6]PW[A\\\nB]C[ignore \\] \\[];W[31hefe])").unwrap();
    assert_eq!(m.players[0], "AB");
    for s in [
        "(;GM[6];W[31hefe](;B[42qusu])(;B[52lqqs]))",
        "(;GM[6]AE[a:y]AW[y][m]AB[a];W[31mjmi])",
        "(;GM[6];W[31hefe];W[42hdfd])",
        "(;GM[6];W[double];W[take];W[31hefe])",
        "(;GM[6];W[31he])",
        "(;GM[6]AE[a:y]AW[a];W[21azaz])",
        "(;GM[6]AE[a:y]AW[a][a][a][a][a][a][a][a][a][a][a][a][a][a][a][a];W[11azazazaz])",
    ] {
        assert!(parse_sgf(s).is_err(), "unexpectedly accepted {s}");
    }
    let valid = "(;GM[6];W[31hefe])";
    for end in 0..valid.len() {
        assert!(parse_sgf(&valid[..end]).is_err());
    }
}

#[test]
fn malformed_and_unsupported_inputs_are_errors() {
    for s in [
        "",
        "(;GM[1];B[aa])",
        "(;GM[6]RU[Nackgammon];W[31hefe])",
        "(;GM[6];W[31hefe]",
        "(;GM[6];W[77hefe])",
        "(;GM[6];W[31az])",
        "(;GM[6];W[31])",
    ] {
        assert!(parse_sgf(s).is_err(), "unexpectedly accepted: {s}");
    }
    assert!(xg_to_sgf(b"invalid").is_err());
}

#[test]
fn compact_xg_runs_interleave_bar_entries_before_continuing() {
    let mut board = [0; 26];
    board[25] = 2;
    board[1] = -1;
    let steps = sgf::expand_xg_play(&mut board, 0, [3, 3], &[(25, 19), (25, 19)]).unwrap();
    assert_eq!(&steps[..2], &[(25, 22), (25, 22)]);
    assert_eq!(steps.len(), 4);
    assert_eq!(board[25], 0);
    assert_eq!(board[19], 2);
}

#[test]
fn compact_xg_runs_do_not_invent_intermediate_hits() {
    let mut board = [0; 26];
    board[13] = 1;
    board[8] = -1;
    let steps = sgf::expand_xg_play(&mut board, 0, [5, 1], &[(13, 7)]).unwrap();
    assert_eq!(steps, vec![(13, 12), (12, 7)]);
    assert_eq!(board[8], -1);
    assert_eq!(board[0], 0);
    assert_eq!(board[7], 1);
    board[13] = 1;
    board[7] = 0;
    board[12] = -2;
    let before = board;
    assert!(sgf::expand_xg_play(&mut board, 0, [5, 1], &[(13, 7)]).is_err());
    assert_eq!(board, before);
}

#[test]
fn compact_xg_runs_keep_endpoint_hits_for_both_players() {
    for player in [0, 1] {
        let mut local = [0; 26];
        local[13] = 1;
        local[7] = -1;
        let mut board = if player == 0 {
            local
        } else {
            std::array::from_fn(|i| -local[25 - i])
        };
        let steps = sgf::expand_xg_play(&mut board, player, [5, 1], &[(13, 7)]).unwrap();
        assert_eq!(steps.len(), 2);
        let after = if player == 0 {
            board
        } else {
            std::array::from_fn(|i| -board[25 - i])
        };
        assert_eq!(after[13], 0);
        assert_eq!(after[7], 1);
        assert_eq!(after[0], -1);
    }
}

#[test]
fn compact_xg_bearing_off_and_single_die_priority_follow_the_rules() {
    let mut board = [0; 26];
    board[6] = 1;
    board[24] = -1;
    let steps = sgf::expand_xg_play(&mut board, 0, [5, 4], &[(6, 0)]).unwrap();
    assert_eq!(steps.len(), 2);
    assert!(!board.iter().any(|n| *n > 0));
    let mut board = [0; 26];
    board[25] = 1;
    board[14] = -2;
    let before = board;
    assert!(sgf::expand_xg_play(&mut board, 0, [6, 5], &[(25, 20)]).is_err());
    assert_eq!(board, before);
}

#[test]
fn winning_bearoff_can_end_before_every_playable_die_is_used() {
    for (color, move_value) in [("W", "42cz"), ("B", "42vz")] {
        let setup = if color == "W" {
            "AW[c]AB[x]"
        } else {
            "AB[v]AW[a]"
        };
        let input = format!("(;GM[6]AE[a:y]{setup};{color}[{move_value}])");
        let imported = parse_sgf(&input).unwrap();
        assert_eq!(imported.positions.len(), 1);
        assert_eq!(imported.positions[0].notation, "3/off");
    }
    let mut board = [0; 26];
    board[3] = 1;
    board[24] = -1;
    assert_eq!(
        sgf::expand_xg_play(&mut board, 0, [4, 2], &[(3, 0)]).unwrap(),
        vec![(3, 0)]
    );
    assert!(!board.iter().any(|n| *n > 0));
    assert!(parse_sgf("(;GM[6]AE[a:y]AW[c][c]AB[x];W[42cz])").is_err());
}
