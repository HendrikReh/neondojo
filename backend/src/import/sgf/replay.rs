use super::parser::{Node, number, one, parse_trees};
use crate::backgammon::{apply_checker_turn, ids, initial_board, local_board, valid_board};
use crate::import::{ImportedMatch, ImportedPosition};
use anyhow::{Context, Result, bail, ensure};

fn points(value: &str) -> Result<Vec<usize>> {
    let a = value.as_bytes();
    match a {
        [p] if (b'a'..=b'y').contains(p) => Ok(vec![usize::from(*p - b'a' + 1)]),
        [p, b':', q] if (b'a'..=b'y').contains(p) && (p..=&b'y').contains(&q) => {
            Ok((usize::from(*p - b'a' + 1)..=usize::from(*q - b'a' + 1)).collect())
        }
        _ => bail!("SGF: invalid setup point {value}"),
    }
}

pub(crate) fn setup(node: &Node, board: &mut [i32; 26]) -> Result<()> {
    if let Some(values) = node.get("AE") {
        for value in values {
            for p in points(value)? {
                board[p] = 0;
                if p == 25 {
                    board[0] = 0;
                }
            }
        }
    }
    for (name, sign) in [("AW", 1), ("AB", -1)] {
        if let Some(values) = node.get(name) {
            for value in values {
                for p in points(value)? {
                    let p = if p == 25 && sign == -1 { 0 } else { p };
                    ensure!(board[p] * sign >= 0, "SGF: both colors occupy point {p}");
                    board[p] += sign;
                }
            }
        }
    }
    valid_board(board).context("SGF: invalid board setup")
}

/// Validate and apply one SGF checker play. Coordinates in SGF are absolute;
/// notation is always from the moving player's perspective.
pub(crate) fn play(board: &mut [i32; 26], player: i32, value: &str) -> Result<([i32; 2], String)> {
    let bytes = value.as_bytes();
    ensure!(
        (2..=10).contains(&bytes.len()) && bytes.len().is_multiple_of(2),
        "SGF: malformed checker move {value}"
    );
    ensure!(
        bytes[..2].iter().all(|d| (b'1'..=b'6').contains(d)),
        "SGF: invalid dice in {value}"
    );
    let dice = [i32::from(bytes[0] - b'0'), i32::from(bytes[1] - b'0')];
    let mut pairs = Vec::new();
    let mut notation = Vec::new();
    let mut local = local_board(board, player);
    for pair in bytes[2..].chunks_exact(2) {
        let point = |c: u8| -> Result<usize> {
            match c {
                b'a'..=b'x' => Ok(if player == 0 {
                    usize::from(c - b'a' + 1)
                } else {
                    usize::from(b'x' - c + 1)
                }),
                b'y' => Ok(25),
                b'z' => Ok(0),
                _ => bail!("SGF: invalid move coordinate"),
            }
        };
        let from = point(pair[0])?;
        let to = point(pair[1])?;
        let hit = to > 0 && to < 25 && local[to] == -1;
        let label = |p: usize| {
            if p == 25 {
                "bar".to_owned()
            } else if p == 0 {
                "off".to_owned()
            } else {
                p.to_string()
            }
        };
        notation.push(format!(
            "{}/{}{}",
            label(from),
            label(to),
            if hit { "*" } else { "" }
        ));
        pairs.push((from, to));
        // Intermediate board is only for hit notation; apply_pairs validates all steps.
        if (1..=25).contains(&from) && to < 25 {
            local[from] -= 1;
            if to > 0 {
                if local[to] == -1 {
                    local[0] -= 1;
                    local[to] = 0;
                }
                local[to] += 1;
            }
        }
    }
    *board = apply_checker_turn(board, player, dice, &pairs)
        .with_context(|| format!("SGF: illegal or incomplete checker move {value}"))?;
    Ok((
        dice,
        if notation.is_empty() {
            "Cannot move".to_owned()
        } else {
            notation.join(" ")
        },
    ))
}

/// Validated state for every game, including games without checker plays.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GameContext {
    pub length: i32,
    pub scores: [i32; 2],
    pub crawford: bool,
    pub jacoby: bool,
    pub cube_enabled: bool,
    pub decision_ids: Vec<(String, String)>,
}

/// An actual played source event and its state before the action. Checker ply
/// links the catalog to the existing canonical position occurrence.
#[derive(Debug)]
pub(crate) struct SourceMove {
    pub game_no: i32,
    pub move_no: i32,
    pub kind: &'static str,
    pub player: i32,
    pub notation: String,
    pub checker_ply: Option<i32>,
    pub board: Vec<i32>,
    pub dice: [i32; 2],
    pub position_id: String,
    pub match_id: String,
    pub after_board: Vec<i32>,
    pub after_match_id: String,
}

pub(crate) fn parse_sgf_source_moves(input: &str) -> Result<(ImportedMatch, Vec<SourceMove>)> {
    let mut moves = Vec::new();
    let (imported, _) = parse_sgf_context(input, false, Some(&mut moves))?;
    Ok((imported, moves))
}

/// The played events together with every game's validated context, including
/// games without checker plays. Exporters replay the stored SGF through this.
pub(crate) fn parse_sgf_events(
    input: &str,
) -> Result<(ImportedMatch, Vec<GameContext>, Vec<SourceMove>)> {
    let mut moves = Vec::new();
    let (imported, contexts) = parse_sgf_context(input, false, Some(&mut moves))?;
    Ok((imported, contexts, moves))
}

pub(crate) fn parse_native_sgf_source_moves(
    input: &str,
) -> Result<(ImportedMatch, Vec<SourceMove>)> {
    let mut moves = Vec::new();
    let (imported, _) = parse_sgf_context(input, true, Some(&mut moves))?;
    Ok((imported, moves))
}

pub fn parse_sgf(input: &str) -> Result<ImportedMatch> {
    let (result, _) = parse_importable_sgf_with_context(input)?;
    Ok(result)
}

/// [`parse_sgf`] that keeps each game's decision context.
pub(crate) fn parse_importable_sgf_with_context(
    input: &str,
) -> Result<(ImportedMatch, Vec<GameContext>)> {
    let (result, contexts) = parse_sgf_with_context(input)?;
    ensure!(
        !result.positions.is_empty(),
        "SGF contains no checker-play decisions"
    );
    Ok((result, contexts))
}

/// Internal analysis accepts an individual empty or cube-only game exported
/// from an otherwise importable match, while public imports still need checkers.
pub(crate) fn parse_sgf_with_context(input: &str) -> Result<(ImportedMatch, Vec<GameContext>)> {
    parse_sgf_context(input, false, None)
}

/// GNUbg 1.05.002 can write PL[W] for a Black setup because its SETBOARD record
/// lacks fPlayer. Only native output may infer turn from the next actual move;
/// callers must compare all resulting context IDs against the strict source.
pub(crate) fn parse_native_sgf_with_context(
    input: &str,
) -> Result<(ImportedMatch, Vec<GameContext>)> {
    parse_sgf_context(input, true, None)
}

/// The Match ID while the double of `doubler` waits for its answer.
fn offered(match_id: &str, doubler: i32) -> Result<String> {
    ids::with_offer_from(match_id, doubler).context("SGF: invalid match ID")
}

fn parse_sgf_context(
    input: &str,
    native_setup_turn: bool,
    mut source_moves: Option<&mut Vec<SourceMove>>,
) -> Result<(ImportedMatch, Vec<GameContext>)> {
    let trees = parse_trees(input)?;
    let mut contexts = Vec::new();
    let mut result_cubes = Vec::with_capacity(trees.len());
    let mut result = ImportedMatch {
        players: ["White".into(), "Black".into()],
        match_length: 0,
        positions: Vec::new(),
        games: Vec::new(),
    };
    let mut previous_scores = [0, 0];
    for (game_index, nodes) in trees.iter().enumerate() {
        let root = &nodes[0];
        ensure!(
            one(root, "GM")? == Some("6"),
            "Only backgammon SGF (GM[6]) is supported"
        );
        if let Some(ff) = one(root, "FF")? {
            ensure!(ff == "4" || ff == "3", "Unsupported SGF version {ff}");
        }
        if let Some(ca) = one(root, "CA")? {
            ensure!(
                ["UTF-8", "UTF8", "ISO-8859-1", "US-ASCII"].contains(&ca.to_uppercase().as_str()),
                "Unsupported SGF character encoding {ca}"
            );
        }
        let mut length = result.match_length;
        let mut scores = previous_scores;
        if let Some(mi) = root.get("MI") {
            for field in mi {
                let (key, value) = field
                    .split_once(':')
                    .context("SGF: malformed MI property")?;
                match key.to_ascii_lowercase().as_str() {
                    "length" => length = number(value, "match length")?,
                    "ws" => scores[0] = number(value, "white score")?,
                    "bs" => scores[1] = number(value, "black score")?,
                    "game" => {
                        number(value, "game number")?;
                    }
                    _ => {}
                }
            }
        }
        if game_index == 0 {
            result.match_length = length;
        }
        ensure!(
            length == result.match_length,
            "SGF collection contains different match lengths"
        );
        ensure!(
            length == 0 || scores.iter().all(|s| *s < length),
            "SGF game starts after the match is over"
        );
        for (i, property) in ["PW", "PB"].iter().enumerate() {
            if let Some(name) = one(root, property)? {
                let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
                if game_index == 0 {
                    result.players[i] = name;
                } else {
                    ensure!(
                        result.players[i] == name,
                        "SGF collection changes player names/colors between games"
                    );
                }
            }
        }
        let mut crawford = false;
        let mut jacoby = false;
        // GNUbg's "set cube use off", e.g. cubeless money duels.
        let mut no_cube = false;
        if let Some(rules) = one(root, "RU")? {
            for rule in rules.split(':') {
                match rule.trim() {
                    "" | "Crawford" | "Backgammon" | "Standard" => {}
                    "CrawfordGame" => crawford = true,
                    "Jacoby" => jacoby = true,
                    "NoCube" => no_cube = true,
                    other => bail!("Unsupported backgammon rule/variant: {other}"),
                }
            }
        }
        ensure!(!crawford || length > 0, "Crawford game in a money session");
        ensure!(!jacoby || length == 0, "Jacoby rule in a point match");
        let mut context = GameContext {
            length,
            scores,
            crawford,
            jacoby,
            cube_enabled: !no_cube,
            decision_ids: Vec::new(),
        };
        let mut board = initial_board();
        let mut cube = 1i32;
        let mut owner = -1;
        let mut pending = None;
        let mut expected_turn = None;
        let mut ended = false;
        let mut dropped_by = None;
        let mut terminal_checker = false;
        let mut terminal_evidence = None;
        let mut ply = 0;
        let mut move_no = 0;
        for (node_index, node) in nodes.iter().enumerate() {
            ensure!(
                node_index == 0
                    || !["GM", "MI", "RU", "PW", "PB"]
                        .iter()
                        .any(|key| node.contains_key(*key)),
                "SGF game metadata outside the root node is not supported"
            );
            setup(node, &mut board)?;
            if let Some(cv) = one(node, "CV")? {
                cube = cv.parse().context("SGF: invalid cube value")?;
                ensure!(
                    cube > 0 && cube <= 32768 && (cube as u32).is_power_of_two(),
                    "SGF: cube must be a power of two"
                );
            }
            let cp = one(node, "CP")?.filter(|v| matches!(*v, "c" | "w" | "b"));
            if let Some(co) = one(node, "CO")?.or(cp) {
                owner = match co {
                    "c" => -1,
                    "w" => 0,
                    "b" => 1,
                    "n" if crawford => -1,
                    _ => bail!("Unsupported SGF cube owner {co}"),
                };
            }
            if native_setup_turn && ["AE", "AW", "AB"].iter().any(|key| node.contains_key(*key)) {
                expected_turn = None;
            } else if let Some(pl) = one(node, "PL")? {
                expected_turn = Some(match pl {
                    "W" => 0,
                    "B" => 1,
                    _ => bail!("SGF: invalid PL"),
                });
            }
            let white = one(node, "W")?;
            let black = one(node, "B")?;
            ensure!(
                white.is_none() || black.is_none(),
                "SGF node contains moves for both players"
            );
            let Some((player, value)) = white.map(|v| (0, v)).or(black.map(|v| (1, v))) else {
                continue;
            };
            ensure!(!ended, "SGF contains moves after a cube drop");
            move_no += 1;
            let before_event = board;
            let pending_before = pending;
            let cube_before = cube;
            let owner_before = owner;
            if matches!(value, "double" | "take" | "drop") {
                ensure!(
                    !no_cube,
                    "SGF cube action in a game played without the cube"
                );
                context.decision_ids.push((
                    ids::position_id(&board, player),
                    ids::match_id(
                        length,
                        scores,
                        cube,
                        owner,
                        crawford,
                        jacoby,
                        player,
                        [0, 0],
                    ),
                ));
            }
            match value {
                "double" => {
                    ensure!(!crawford, "SGF contains a double in a Crawford game");
                    ensure!(pending.is_none(), "Beavers/raccoons are not supported");
                    ensure!(
                        owner == -1 || owner == player,
                        "SGF: player doubles an opponent-owned cube"
                    );
                    ensure!(
                        expected_turn.is_none_or(|turn| turn == player),
                        "SGF: wrong player doubles"
                    );
                    ensure!(cube < 32768, "SGF: cube exceeds GNUbg ID range");
                    pending = Some(player);
                }
                "take" | "drop" => {
                    let doubler = pending
                        .take()
                        .context("SGF: cube response without a double")?;
                    ensure!(
                        player != doubler,
                        "SGF: player responds to their own double"
                    );
                    if value == "take" {
                        cube *= 2;
                        owner = player;
                        expected_turn = Some(doubler);
                    } else {
                        ended = true;
                        dropped_by = Some(player);
                        terminal_evidence = Some((board, cube, owner));
                    }
                }
                _ => {
                    ensure!(pending.is_none(), "SGF: checker move before cube response");
                    ensure!(
                        expected_turn.is_none_or(|turn| turn == player),
                        "SGF: non-alternating checker moves"
                    );
                    let before = board;
                    ensure!(
                        before.iter().any(|n| *n > 0) && before.iter().any(|n| *n < 0),
                        "SGF: checker move after a player has borne off every checker"
                    );
                    let (dice, notation) = play(&mut board, player, value).with_context(|| {
                        format!("game {}, checker ply {}", game_index + 1, ply + 1)
                    })?;
                    terminal_checker =
                        !board.iter().any(|n| *n > 0) || !board.iter().any(|n| *n < 0);
                    if terminal_checker {
                        terminal_evidence = Some((board, cube, owner));
                    }
                    ply += 1;
                    context.decision_ids.push((
                        ids::position_id(&before, player),
                        ids::match_id(length, scores, cube, owner, crawford, jacoby, player, dice),
                    ));
                    result.positions.push(ImportedPosition {
                        cube_enabled: !no_cube,
                        game_no: game_index as i32 + 1,
                        ply,
                        player,
                        dice,
                        board: before.to_vec(),
                        position_id: ids::position_id(&before, player),
                        match_id: ids::match_id(
                            length, scores, cube, owner, crawford, jacoby, player, dice,
                        ),
                        notation,
                    });
                    expected_turn = Some(1 - player);
                }
            }
            if let Some(moves) = source_moves.as_deref_mut() {
                let checker = !matches!(value, "double" | "take" | "drop");
                let (dice, notation) = if checker {
                    let position = result
                        .positions
                        .last()
                        .context("SGF: missing checker snapshot")?;
                    (position.dice, position.notation.clone())
                } else {
                    (
                        [0, 0],
                        match value {
                            "double" => "Double",
                            "take" => "Take",
                            _ => "Drop",
                        }
                        .to_owned(),
                    )
                };
                let mut match_id = ids::match_id(
                    length,
                    scores,
                    cube_before,
                    owner_before,
                    crawford,
                    jacoby,
                    player,
                    dice,
                );
                if let Some(doubler) = pending_before {
                    match_id = offered(&match_id, doubler)?;
                }
                let after_turn = if checker {
                    1 - player
                } else {
                    pending.or(pending_before).unwrap_or(player)
                };
                let mut after_match_id = ids::match_id(
                    length,
                    scores,
                    cube,
                    owner,
                    crawford,
                    jacoby,
                    after_turn,
                    [0, 0],
                );
                if let Some(doubler) = pending {
                    after_match_id = offered(&after_match_id, doubler)?;
                }
                moves.push(SourceMove {
                    game_no: game_index as i32 + 1,
                    move_no,
                    kind: if checker { "checker" } else { "cube" },
                    player,
                    notation,
                    checker_ply: checker.then_some(ply),
                    board: before_event.to_vec(),
                    dice,
                    position_id: ids::position_id(&before_event, pending_before.unwrap_or(player)),
                    match_id,
                    after_board: board.to_vec(),
                    after_match_id,
                });
            }
        }
        // GNUbg writes MOVE_RESIGN only as the result's R suffix and restores
        // it as a final source event after replaying the explicit move nodes.
        // Preserve the existing checker and analysis-context import contracts.
        if let Some(moves) = source_moves.as_deref_mut()
            && let Some((winner, points)) = one(root, "RE")?.and_then(|r| r.split_once('+'))
            && points.ends_with(['R', 'r'])
        {
            ensure!(
                pending.is_none() && !ended,
                "SGF: resignation after cube offer or drop"
            );
            let player = match winner {
                "W" => 1,
                "B" => 0,
                _ => bail!("SGF: invalid resignation winner"),
            };
            moves.push(SourceMove {
                game_no: game_index as i32 + 1,
                move_no: move_no + 1,
                kind: "resign",
                player,
                notation: "Resign".to_owned(),
                after_board: board.to_vec(),
                after_match_id: ids::match_id(
                    length,
                    scores,
                    cube,
                    owner,
                    crawford,
                    jacoby,
                    player,
                    [0, 0],
                ),
                checker_ply: None,
                board: board.to_vec(),
                dice: [0, 0],
                position_id: ids::position_id(&board, player),
                match_id: ids::match_id(
                    length,
                    scores,
                    cube,
                    owner,
                    crawford,
                    jacoby,
                    player,
                    [0, 0],
                ),
            });
        }
        let (result_board, result_cube, result_owner) =
            terminal_evidence.unwrap_or((board, cube, owner));
        let outcome = crate::game_result::derive(
            &result_board,
            result_cube,
            jacoby && result_owner == -1,
            scores,
            terminal_checker,
            dropped_by,
            one(root, "RE")?,
        );
        result_cubes.push(result_cube);
        result.games.push(crate::model::MatchGame {
            cube_enabled: Some(!no_cube),
            result: outcome.clone(),
            game_no: game_index as i32 + 1,
            position_count: ply,
            white_score: scores[0],
            black_score: scores[1],
            crawford,
        });
        contexts.push(context);
        previous_scores = scores;
        if let Some(re) = one(root, "RE")?
            && let Some((winner, points)) = re.split_once('+')
        {
            let digits = points
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            if !digits.is_empty() {
                let n = number(&digits, "result points")?;
                let side = match winner {
                    "W" => 0,
                    "B" => 1,
                    _ => bail!("SGF: invalid winner"),
                };
                previous_scores[side] = previous_scores[side]
                    .checked_add(n)
                    .context("Score overflow")?;
            }
        }
    }
    // Read only explicit next-root score fields. Inferred results must never
    // feed previous_scores or alter position/match IDs during source replay.
    for (index, (game, (next, cube))) in result
        .games
        .iter_mut()
        .zip(trees.iter().skip(1).zip(result_cubes))
        .enumerate()
    {
        if game.result.is_none()
            && !trees[index][0].contains_key("RE")
            && consecutive_declared_games(&trees[index][0], &next[0])
            && let Some(scores_after) = explicit_scores(&next[0])
        {
            game.result = crate::game_result::from_following_scores(
                [game.white_score, game.black_score],
                scores_after,
                cube,
            );
        }
    }
    Ok((result, contexts))
}

fn consecutive_declared_games(current: &Node, next: &Node) -> bool {
    let game_number = |root: &Node| -> Option<i32> {
        root.get("MI")?.iter().rev().find_map(|field| {
            let (key, value) = field.split_once(':')?;
            key.eq_ignore_ascii_case("game")
                .then(|| value.trim().parse().ok())
                .flatten()
        })
    };
    match (game_number(current), game_number(next)) {
        (Some(current), Some(next)) => current.checked_add(1) == Some(next),
        _ => true,
    }
}

fn explicit_scores(root: &Node) -> Option<[i32; 2]> {
    let mut scores = [None, None];
    for field in root.get("MI")? {
        let (key, value) = field.split_once(':')?;
        let side = match key.to_ascii_lowercase().as_str() {
            "ws" => 0,
            "bs" => 1,
            _ => continue,
        };
        let value = value.trim().parse::<i32>().ok()?;
        if scores[side].is_some_and(|previous| previous != value) {
            return None;
        }
        scores[side] = Some(value);
    }
    Some([scores[0]?, scores[1]?])
}
