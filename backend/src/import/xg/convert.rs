use super::{
    analysis::{analysis_property, checker_analysis, cube_analysis},
    container::{
        MATCH_MAGIC, RECORD_BYTES, bool_at, header_text, i32_at, main_records, match_context,
        u32_at,
    },
};
use crate::import::{
    ConvertedMatch,
    match_context::MatchContext,
    parse_sgf,
    sgf::{coord::encode_point, escape_value},
};
use crate::model::{ImportWarning, ImportWarningKind};
use anyhow::{Context, Result, bail, ensure};

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
    let player = if active == 1 { 0 } else { 1 };
    let relative = match point {
        -6..=-1 => 0,
        0..=24 => (point + 1) as usize,
        _ => bail!("XG: invalid checker coordinate {point}"),
    };
    encode_point(relative, player)
}

struct XgConverter {
    version: i32,
    length: i32,
    crawford_rule: bool,
    jacoby: bool,
    players: [String; 2],
    context: MatchContext,
    warnings: Vec<ImportWarning>,
    out: String,
    in_game: bool,
    game_in_progress: bool,
    saw_footer: bool,
    match_decided: bool,
    game_count: i32,
    checker_no: i32,
    cube: i32,
    owner: i32, // XG player +/-1; zero is centered.
    game_crawford: bool,
    game_scores: [i32; 2],
    root_end: usize,
    board: [i32; 26],
    pending_cube_analysis: Option<serde_json::Value>,
    first_checker_record: Option<usize>,
}

impl XgConverter {
    fn from_header(header: &[u8]) -> Result<Self> {
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
        Ok(Self {
            version,
            length,
            crawford_rule: bool_at(header, 100)?,
            jacoby: bool_at(header, 101)?,
            players: [
                header_text(header, 880, 9, 40)?,
                header_text(header, 1138, 50, 40)?,
            ],
            context: match_context(header),
            warnings: Vec::new(),
            out: String::new(),
            in_game: false,
            game_in_progress: false,
            saw_footer: false,
            match_decided: false,
            game_count: 0,
            checker_no: 0,
            cube: 1,
            owner: 0,
            game_crawford: false,
            game_scores: [0, 0],
            root_end: 0,
            board: crate::backgammon::initial_board(),
            pending_cube_analysis: None,
            first_checker_record: None,
        })
    }

    fn handle_record(&mut self, bytes: &[u8], index: usize, record: &[u8]) -> Result<()> {
        ensure!(!self.saw_footer, "XG: records after match footer");
        match record[8] {
            1 => self.handle_game_header(bytes, index, record),
            2 => self.handle_cube_record(index, record),
            3 => self.handle_checker_record(index, record),
            4 => self.handle_game_footer(record),
            5 => self.handle_match_footer(),
            kind => bail!("Unsupported XG record type {kind} at record {index}"),
        }
    }

    fn handle_game_header(&mut self, bytes: &[u8], index: usize, record: &[u8]) -> Result<()> {
        // An unfinished game followed by the next game was abandoned
        // in the source. Keep its moves without inventing a result.
        if self.in_game && self.game_in_progress {
            self.out.push_str(")\n");
            self.in_game = false;
            self.warnings.push(ImportWarning::at(
                ImportWarningKind::UnfinishedGame,
                self.game_count,
                None,
            ));
        }
        ensure!(!self.in_game, "XG: game header before previous game footer");
        // Cumulative money-session totals do not enter GNUbg equity
        // calculations; Match IDs use zero scores for money games.
        let white_score = if self.length == 0 {
            0
        } else {
            i32_at(record, 12)?
        };
        let black_score = if self.length == 0 {
            0
        } else {
            i32_at(record, 16)?
        };
        ensure!(
            white_score >= 0 && black_score >= 0,
            "XG: negative match score is not supported"
        );
        self.game_scores = [white_score, black_score];
        self.game_crawford = bool_at(record, 20)?;
        self.game_in_progress = bool_at(record, 52)?;
        // The first checker and cube records hold the game's actual
        // starting board and cube; transcripts can begin mid-game.
        let game_records = bytes[(index + 1) * RECORD_BYTES..]
            .as_chunks::<RECORD_BYTES>()
            .0
            .iter()
            .enumerate()
            .take_while(|(_, r)| !matches!(r[8], 1 | 4 | 5));
        let first_checker = game_records.clone().find(|(_, r)| r[8] == 3);
        self.first_checker_record = first_checker.map(|(offset, _)| index + 1 + offset);
        let start_board = first_checker
            .map(|(_, r)| xg_board(r, 9))
            .unwrap_or_else(crate::backgammon::initial_board);
        let start_cube = match game_records.map(|(_, r)| r).find(|r| matches!(r[8], 2 | 3)) {
            Some(r) => cube_state(i32_at(r, if r[8] == 2 { 32 } else { 108 })?)?,
            None => (1, 0),
        };
        // XG keeps the opening setup here when the first checker record
        // was edited; any other stored board must be the actual start.
        let initial = xg_board(record, 21);
        ensure!(
            initial.iter().all(|n| *n == 0)
                || initial == crate::backgammon::initial_board()
                || initial == start_board,
            "XG: game {} header position does not match its first checker record",
            self.game_count + 1
        );
        ensure!(
            self.version < 26 || i32_at(record, 64)? == 0,
            "XG automatic doubles are not supported"
        );
        let mut rules = Vec::new();
        if self.crawford_rule {
            rules.push("Crawford");
        }
        if self.game_crawford {
            rules.push("CrawfordGame");
        }
        if self.jacoby && self.length == 0 {
            rules.push("Jacoby");
        }
        self.out.push_str(&format!(
            "(;FF[4]GM[6]CA[UTF-8]AP[Neondojo:XG2]PW[{}]PB[{}]MI[length:{}][game:{}][ws:{white_score}][bs:{black_score}]",
            escape_value(&self.players[0]),
            escape_value(&self.players[1]),
            self.length,
            self.game_count,
        ));
        if self.game_count == 0 {
            self.context.write_sgf(&mut self.out);
        }
        if !rules.is_empty() {
            self.out.push_str(&format!("RU[{}]", rules.join(":")));
        }
        self.root_end = self.out.len();
        self.in_game = true;
        self.checker_no = 0;
        self.game_count += 1;
        self.board = start_board;
        (self.cube, self.owner) = start_cube;
        self.pending_cube_analysis = None;
        if self.board != crate::backgammon::initial_board() || start_cube != (1, 0) {
            let turn = first_checker
                .map(|(_, r)| i32_at(r, 64).and_then(color))
                .transpose()?;
            self.out
                .push_str(&setup_node(&self.board, turn, Some(start_cube))?);
            self.warnings.push(ImportWarning::at(
                ImportWarningKind::CustomStartPosition,
                self.game_count,
                None,
            ));
        }
        Ok(())
    }

    fn handle_cube_record(&mut self, index: usize, record: &[u8]) -> Result<()> {
        ensure!(self.in_game, "XG: cube record outside game");
        ensure!(
            !bool_at(record, 296)?,
            "XG edited cube positions are not supported"
        );
        ensure!(
            i32_at(record, 260)? == 0,
            "XG contains a cube record marked invalid"
        );
        ensure!(
            self.version < 27 || i32_at(record, 300)? == 0,
            "XG automatic doubles are not supported"
        );
        let active = i32_at(record, 12)?;
        let action = i32_at(record, 16)?;
        ensure!((-2..=1).contains(&action), "XG: invalid double action");
        // XG can store an unavailable-cube placeholder before the opening
        // roll with no actor. It is not a played cube decision.
        if active == 0 && action == -2 {
            return Ok(());
        }
        let c = color(active)?;
        let analysis = cube_analysis(record, index)?;
        if action == 1 {
            ensure!(!self.game_crawford, "XG: double in a Crawford game");
            ensure!(
                self.owner == 0 || self.owner == active,
                "XG: double by player without cube ownership"
            );
            let take = i32_at(record, 20)?;
            ensure!(
                (0..=1).contains(&take),
                "XG beavers/raccoons and unanswered doubles are not supported"
            );
            self.out.push_str(&format!("\n;{c}[double]"));
            if let Some(ref value) = analysis {
                analysis_property(&mut self.out, "XGC", value)?;
            }
            self.out.push_str(&format!(
                ";{}[{}]",
                color(-active)?,
                if take == 1 { "take" } else { "drop" }
            ));
            if let Some(ref value) = analysis {
                analysis_property(&mut self.out, "XGC", value)?;
            }
            if take == 1 {
                self.cube *= 2;
                ensure!(self.cube <= 32768, "XG: cube value exceeds GNUbg range");
                self.owner = -active;
            }
        } else if action == 0 || action == -1 {
            self.pending_cube_analysis = analysis;
        }
        Ok(())
    }

    fn handle_checker_record(&mut self, index: usize, record: &[u8]) -> Result<()> {
        ensure!(self.in_game, "XG: checker record outside game");
        // An edit of the first checker record defines the game's
        // starting setup, which is imported from its recorded board.
        ensure!(
            !bool_at(record, 2528)? || self.first_checker_record == Some(index),
            "XG edited checker positions are not supported (game {}, record {index})",
            self.game_count
        );
        let invalid = i32_at(record, 2480)?;
        // Code 2 is a played move that XG accepted although it is not
        // legal; XG keeps the position after it. Others stay rejected.
        ensure!(
            invalid == 0 || invalid == 2,
            "XG: checker record marked invalid (code {invalid}) in game {}, checker ply {} (record {index})",
            self.game_count,
            self.checker_no + 1
        );
        ensure!(
            xg_board(record, 9) == self.board,
            "XG: recorded position does not follow from the previous plays in game {}, checker ply {} (record {index})",
            self.game_count,
            self.checker_no + 1
        );
        ensure!(
            self.version < 27 || i32_at(record, 2540)? == 0,
            "XG automatic doubles are not supported"
        );
        if !bool_at(record, 2308)? {
            return Ok(());
        }
        self.checker_no += 1;
        let active = i32_at(record, 64)?;
        let c = color(active)?;
        let player = if active == 1 { 0 } else { 1 };
        // Unlike DataMoves.CubePos, CubeA uses absolute player1 /
        // player2 ownership. Confirmed against the official sample.
        ensure!(
            cube_state(i32_at(record, 108)?)? == (self.cube, self.owner),
            "XG: inconsistent cube context at record {index}"
        );
        if invalid == 2 {
            // PositionEnd is from the mover's perspective. Continue
            // from it; the illegal play is not a checker decision, so
            // its native analysis and the preceding no-double
            // analysis have no imported decision to attach to.
            self.board = crate::backgammon::local_board(&xg_board(record, 35), player);
            self.out
                .push_str(&setup_node(&self.board, Some(color(-active)?), None)?);
            self.pending_cube_analysis = None;
            self.warnings.push(ImportWarning::at(
                ImportWarningKind::IllegalPlayReplaced,
                self.game_count,
                Some(self.checker_no),
            ));
            return Ok(());
        }
        let d1 = i32_at(record, 100)?;
        let d2 = i32_at(record, 104)?;
        ensure!(
            (1..=6).contains(&d1) && (1..=6).contains(&d2),
            "XG: invalid dice"
        );
        let runs = checker_runs(record, active)?;
        let steps = crate::backgammon::expand_joined_play(&mut self.board, player, [d1, d2], &runs)
            .with_context(|| {
                format!(
                    "XG checker-play validation failed at record {index} (game {})",
                    self.game_count
                )
            })?;
        self.out.push_str(&format!("\n;{c}[{d1}{d2}"));
        for (from, to) in steps {
            self.out.push(encode_point(from, player)?);
            self.out.push(encode_point(to, player)?);
        }
        self.out.push(']');
        if let Some(value) = checker_analysis(record, index)? {
            analysis_property(&mut self.out, "XGA", &value)?;
        }
        if let Some(mut value) = self.pending_cube_analysis.take() {
            if value.get("played").is_none() {
                value["played"] = serde_json::json!("No double");
            }
            analysis_property(&mut self.out, "XGC", &value)?;
        }
        Ok(())
    }

    fn handle_game_footer(&mut self, record: &[u8]) -> Result<()> {
        ensure!(self.in_game, "XG: game footer outside game");
        let final_scores = [i32_at(record, 12)?, i32_at(record, 16)?];
        self.match_decided = self.length > 0 && final_scores.iter().any(|s| *s >= self.length);
        // tsFooterGame contains the actual final scores. Pointswon can
        // hold producer sentinels for match resignations (e.g. 1998),
        // so derive points only from a consistent, one-sided delta.
        if self.length > 0 && !self.game_in_progress {
            let side = match i32_at(record, 24)? {
                1 => Some(0),
                -1 => Some(1),
                _ => None,
            };
            if let Some(side) = side
                && final_scores[1 - side] == self.game_scores[1 - side]
                && let Some(points) = final_scores[side].checked_sub(self.game_scores[side])
                && (1..=32767).contains(&points)
            {
                let color = if side == 0 { 'W' } else { 'B' };
                let termination = i32_at(record, 32)?;
                let resignation = if (100..1000).contains(&termination) {
                    "R"
                } else {
                    ""
                };
                self.out
                    .insert_str(self.root_end, &format!("RE[{color}+{points}{resignation}]"));
            }
        }
        self.out.push_str(")\n");
        self.in_game = false;
        Ok(())
    }

    fn handle_match_footer(&mut self) -> Result<()> {
        ensure!(!self.in_game, "XG: match footer before game footer");
        self.saw_footer = true;
        Ok(())
    }

    fn finish(mut self) -> Result<ConvertedMatch> {
        // XG can save between games, or during a game whose header explicitly
        // carries InProgress. Close only that declared unfinished game; a game
        // marked complete without its footer remains an incomplete record stream.
        if self.in_game && self.game_in_progress {
            self.out.push_str(")\n");
            self.in_game = false;
        }
        // A point match that reached its target still needs its match footer.
        // Ongoing matches and money sessions do not have a match winner/footer.
        // Archive CRCs and exact record bounds were checked before parsing records.
        ensure!(
            (self.saw_footer || !self.match_decided) && !self.in_game && self.game_count > 0,
            "XG: incomplete match record stream"
        );
        // Replay validates bars, hits, legal die distances, bearing off, and turns.
        parse_sgf(&self.out).context("XG checker-play validation failed")?;
        Ok(ConvertedMatch {
            sgf: self.out,
            warnings: self.warnings,
        })
    }
}

fn checker_runs(record: &[u8], active: i32) -> Result<Vec<(usize, usize)>> {
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
    Ok(runs)
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
pub fn convert_xg(input: &[u8]) -> Result<ConvertedMatch> {
    let bytes = main_records(input)?;
    ensure!(
        bytes.len() >= RECORD_BYTES * 2 && bytes.len().is_multiple_of(RECORD_BYTES),
        "XG: invalid 2560-byte record layout"
    );
    let mut converter = XgConverter::from_header(&bytes[..RECORD_BYTES])?;
    for (index, record) in bytes
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
        .enumerate()
        .skip(1)
    {
        converter.handle_record(&bytes, index, record)?;
    }
    converter.finish()
}
