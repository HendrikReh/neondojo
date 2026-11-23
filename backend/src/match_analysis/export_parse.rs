use super::*;

/// GNUbg's own denominator excludes checker plays with at most one legal move
/// and cube choices that are neither close nor actual. Its statistics also use
/// native normalized losses, rather than sums reconstructed from move exports.
/// See the GNUbg manual, Statistics / Overall rating, and analysis.c GetStatistics.
/// https://www.gnu.org/software/gnubg/manual/gnubg.html#gnubg-matches_review_stat_overall
pub(super) fn parse_native_statistics(
    export: &str,
    game_no: i32,
    decisions: &[Decision],
) -> Option<[NativePlayerStatistics; 2]> {
    static HEADER: OnceLock<Regex> = OnceLock::new();
    let header =
        HEADER.get_or_init(|| Regex::new(r"(?m)^Game statistics for game (\d+)\r?$").unwrap());
    let mut sections = header.captures_iter(export);
    let section = sections.next()?;
    if section[1].parse::<i32>().ok()? != game_no
        || sections.next().is_some()
        || decisions
            .iter()
            .any(|d| d.evaluation_status == "unavailable")
    {
        return None;
    }
    let statistics = &export[section.get(0)?.end()..];
    let (checker, cube_and_overall) = statistics.split_once("Cube statistics")?;
    let (cube, overall) = cube_and_overall.split_once("Overall statistics")?;
    let counts = |section: &str, label: &str| -> Option<[i32; 2]> {
        let mut values = section
            .lines()
            .filter_map(|line| line.trim().strip_prefix(label));
        let row = values.next()?;
        if values.next().is_some() {
            return None;
        }
        let values: Vec<_> = row.split_whitespace().map(str::parse::<i32>).collect();
        match values.as_slice() {
            [Ok(white), Ok(black)] if *white >= 0 && *black >= 0 => Some([*white, *black]),
            _ => None,
        }
    };
    let total_moves = counts(checker, "Total moves")?;
    let unforced = counts(checker, "Unforced moves")?;
    let total_cube = counts(cube, "Total cube decisions")?;
    let close_cube = counts(cube, "Close or actual cube decisions")?;
    static LOSS: OnceLock<Regex> = OnceLock::new();
    let loss = LOSS.get_or_init(|| {
        Regex::new(
        // The first value is the normalized loss; the bracket holds match
        // winning chances in a match and points in a money game.
        r"(?m)^Error total EMG \((?:MWC|Points)\)\s+(-\d+\.\d+)\s+\([^\n)]*\)\s+(-\d+\.\d+)\s+\([^\n)]*\)\s*$"
    ).unwrap()
    });
    let mut rows = loss.captures_iter(overall);
    let row = rows.next()?;
    if rows.next().is_some() {
        return None;
    }
    let losses = [-row[1].parse::<f64>().ok()?, -row[2].parse::<f64>().ok()?];
    let mut players = Vec::with_capacity(2);
    for player in 0..2 {
        let count = |kind| {
            decisions
                .iter()
                .filter(|d| d.player == player as i32 && d.kind == kind)
                .count()
        };
        if total_moves[player] as usize != count("checker")
            || total_cube[player] as usize != count("cube")
            || unforced[player] > total_moves[player]
            || close_cube[player] > total_cube[player]
            || !losses[player].is_finite()
            || losses[player] < 0.0
        {
            return None;
        }
        players.push(NativePlayerStatistics {
            total_checker_moves: total_moves[player],
            total_cube_decisions: total_cube[player],
            unforced_checker_moves: unforced[player],
            close_cube_decisions: close_cube[player],
            normalized_equity_loss: losses[player],
        });
    }
    players.try_into().ok()
}

pub(super) fn moves(nodes: &[Node]) -> Result<Vec<(i32, &str, &Node)>> {
    let mut moves = Vec::new();
    for node in nodes {
        for (player, key) in [(0, "W"), (1, "B")] {
            if let Some(values) = node.get(key) {
                ensure!(values.len() == 1, "Native SGF move has multiple values");
                moves.push((player, values[0].as_str(), node));
            }
        }
    }
    Ok(moves)
}

pub(super) fn cube_decision(block: &str, player: i32, played: &str) -> Result<Decision> {
    let crate::engine::CubeEquities {
        mut no_double,
        mut take,
        mut pass,
        mut probabilities,
    } = crate::engine::parse_cube_equities(block)?;
    // A pass loses one pre-offer cube unit for the responder. Older
    // GNUbg exports use the doubler's perspective, where it is positive.
    if matches!(played, "take" | "drop") && pass > 0.0 {
        no_double = -no_double;
        take = -take;
        pass = -pass;
        probabilities = probabilities.map(crate::engine::flip_probabilities);
    }
    let mut result = evaluated_cube_decision(player, played, no_double, take, pass)?;
    if let Some(evaluation) = &mut result.cube_evaluation {
        evaluation.probabilities = probabilities;
    }
    Ok(result)
}

#[cfg(test)]
pub(super) fn parse_game(
    sgf: &str,
    game_no: i32,
    native: &str,
    text: &str,
) -> Result<Vec<Decision>> {
    parse_source_game(
        &AnalysisSource::for_game(sgf, game_no)?,
        game_no,
        native,
        text,
    )
}

/// Check GNUbg's analysis of one game against the played match it was given.
pub(super) fn parse_source_game(
    source: &AnalysisSource,
    game_no: i32,
    native: &str,
    text: &str,
) -> Result<Vec<Decision>> {
    let selected = source
        .trees
        .get(usize::try_from(game_no - 1)?)
        .context("Game number outside match")?;
    let (native_match, native_contexts) =
        crate::import::sgf::parse_native_sgf_with_context(native)?;
    let native_positions = native_match.positions;
    let native = parse_trees(native)?;
    ensure!(native.len() == 1, "GNUbg did not export exactly one game");
    let source_moves = moves(selected)?;
    let native_moves = moves(&native[0])?;
    ensure!(
        source_moves.len() == native_moves.len(),
        "GNUbg truncated the selected game"
    );
    for ((player, played, _), (native_player, native_played, _)) in
        source_moves.iter().zip(&native_moves)
    {
        ensure!(
            player == native_player && played == native_played,
            "GNUbg changed the played move sequence"
        );
    }
    ensure!(
        source.contexts.get(usize::try_from(game_no - 1)?) == native_contexts.first(),
        "GNUbg changed the game score, rules, or checker/cube decision context"
    );
    let positions: Vec<_> = source
        .imported
        .positions
        .iter()
        .filter(|p| p.game_no == game_no)
        .collect();
    ensure!(
        positions.len() == native_positions.len(),
        "Native game position coverage changed"
    );
    for (original, analyzed) in positions.iter().zip(&native_positions) {
        ensure!(
            original.position_id == analyzed.position_id && original.match_id == analyzed.match_id,
            "GNUbg changed the board, match score, dice, rules or cube context"
        );
    }
    static HEADERS: OnceLock<Regex> = OnceLock::new();
    let headers = HEADERS.get_or_init(|| Regex::new(r"(?m)^Move number (\d+):[^\n]*").unwrap());
    let starts: Vec<_> = headers
        .captures_iter(text)
        .map(|c| (c.get(0).unwrap().start(), c[1].parse::<usize>()))
        .collect();
    ensure!(
        starts.len() == source_moves.len(),
        "GNUbg text export has incomplete move coverage"
    );
    let mut decisions = Vec::new();
    let mut checker_no = 0usize;
    for (index, (player, played, node)) in native_moves.into_iter().enumerate() {
        ensure!(
            starts[index].1.as_ref().is_ok_and(|n| *n == index + 1),
            "Native export move numbering is incomplete"
        );
        let block = &text[starts[index].0..starts.get(index + 1).map_or(text.len(), |s| s.0)];
        if matches!(played, "double" | "take" | "drop") {
            decisions.push(cube_decision(block, player, played)?);
            continue;
        }
        let position = positions
            .get(checker_no)
            .context("Native checker move has no imported position")?;
        checker_no += 1;
        ensure!(position.player == player, "Native checker player mismatch");
        ensure!(
            !requires_cube_analysis(position)? || node.contains_key("DA"),
            "GNUbg omitted an available cube decision"
        );
        if node.contains_key("DA") {
            let mut cube = cube_decision(block, player, "no double")?;
            cube.checker_no = Some(checker_no as i32);
            decisions.push(cube);
        } else {
            ensure!(
                !block.contains("Cube analysis"),
                "Native cube analysis is inconsistent with SGF"
            );
        }
        let mut rows = parse_checker_candidates(block)?;
        let mut checker = if rows.is_empty() {
            ensure!(
                played.len() == 2 && block.lines().any(|l| l.trim() == "*    Cannot move"),
                "GNUbg returned no checker evaluation for an unforced move"
            );
            decision(player, "checker", "Cannot move", "Cannot move", None)
        } else {
            let best = &rows[0].0;
            ensure!(
                rows.iter()
                    .enumerate()
                    .all(|(index, (candidate, _))| candidate.rank as usize == index + 1),
                "Native export has incomplete checker candidate ranks"
            );
            let mut played_rows = rows.iter().filter(|(_, played)| *played);
            let (actual, _) = played_rows
                .next()
                .context("Native export omitted the played checker move")?;
            ensure!(
                played_rows.next().is_none(),
                "Native export marked multiple played checker moves"
            );
            let analysis = node.get("A").context("Native checker analysis missing")?;
            let selected: usize = analysis
                .first()
                .context("Native checker analysis missing")?
                .parse()?;
            ensure!(
                actual.rank as usize == selected + 1,
                "Native actual checker candidate mismatch"
            );
            let mut checker = decision(
                player,
                "checker",
                &actual.notation,
                &best.notation,
                Some((actual.equity, best.equity)),
            );
            retain_checker_paths(
                rows.iter_mut().map(|(candidate, _)| candidate),
                position,
                analysis,
            );
            checker.checker_candidates = rows.into_iter().map(|(candidate, _)| candidate).collect();
            checker
        };
        checker.checker_no = Some(checker_no as i32);
        checker.dice = Some(position.dice);
        decisions.push(checker);
    }
    ensure!(
        checker_no == positions.len(),
        "GNUbg omitted imported checker positions"
    );
    for (index, decision) in decisions.iter_mut().enumerate() {
        decision.decision_no = index as i32 + 1;
    }
    Ok(decisions)
}
