use super::coord::{decode_point, format_step};
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
        let from = decode_point(pair[0], player)?;
        let to = decode_point(pair[1], player)?;
        let hit = to > 0 && to < 25 && local[to] == -1;
        notation.push(format_step(from, to, hit));
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
    source_moves: Option<&mut Vec<SourceMove>>,
) -> Result<(ImportedMatch, Vec<GameContext>)> {
    let trees = parse_trees(input)?;
    replay_trees(&trees, native_setup_turn, source_moves)
}

pub(crate) fn replay_trees(
    trees: &[Vec<Node>],
    native_setup_turn: bool,
    mut source_moves: Option<&mut Vec<SourceMove>>,
) -> Result<(ImportedMatch, Vec<GameContext>)> {
    let mut contexts = Vec::with_capacity(trees.len());
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
        let context = parse_game_header(root, game_index, &mut result, previous_scores)?;
        let mut replay = GameReplay::new(context);
        for (node_index, node) in nodes.iter().enumerate() {
            replay.apply_node_setup(node_index, node, native_setup_turn)?;
            let Some((player, value)) = node_action(node)? else {
                continue;
            };
            replay.apply_action(
                game_index,
                player,
                value,
                &mut result.positions,
                source_moves.as_deref_mut(),
            )?;
        }
        if let Some(moves) = source_moves.as_deref_mut() {
            replay.record_resignation(root, game_index, moves)?;
        }
        let (game, context, result_cube, next_scores) = replay.finish(root, game_index)?;
        result_cubes.push(result_cube);
        result.games.push(game);
        contexts.push(context);
        previous_scores = next_scores;
    }
    infer_results_from_following_scores(&mut result.games, trees, &result_cubes);
    Ok((result, contexts))
}

fn parse_game_header(
    root: &Node,
    game_index: usize,
    result: &mut ImportedMatch,
    previous_scores: [i32; 2],
) -> Result<GameContext> {
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
    Ok(GameContext {
        length,
        scores,
        crawford,
        jacoby,
        cube_enabled: !no_cube,
        decision_ids: Vec::new(),
    })
}

fn node_action(node: &Node) -> Result<Option<(i32, &str)>> {
    let white = one(node, "W")?;
    let black = one(node, "B")?;
    ensure!(
        white.is_none() || black.is_none(),
        "SGF node contains moves for both players"
    );
    Ok(white.map(|v| (0, v)).or(black.map(|v| (1, v))))
}

struct MoveSnapshot {
    board: [i32; 26],
    pending: Option<i32>,
    cube: i32,
    owner: i32,
}

struct GameReplay {
    context: GameContext,
    board: [i32; 26],
    cube: i32,
    owner: i32,
    pending: Option<i32>,
    expected_turn: Option<i32>,
    ended: bool,
    dropped_by: Option<i32>,
    terminal_checker: bool,
    terminal_evidence: Option<([i32; 26], i32, i32)>,
    ply: i32,
    move_no: i32,
}

impl GameReplay {
    fn new(context: GameContext) -> Self {
        Self {
            context,
            board: initial_board(),
            cube: 1,
            owner: -1,
            pending: None,
            expected_turn: None,
            ended: false,
            dropped_by: None,
            terminal_checker: false,
            terminal_evidence: None,
            ply: 0,
            move_no: 0,
        }
    }

    fn match_id(&self, cube: i32, owner: i32, player: i32, dice: [i32; 2]) -> String {
        ids::match_id(
            self.context.length,
            self.context.scores,
            cube,
            owner,
            self.context.crawford,
            self.context.jacoby,
            player,
            dice,
        )
    }

    fn apply_node_setup(
        &mut self,
        node_index: usize,
        node: &Node,
        native_setup_turn: bool,
    ) -> Result<()> {
        ensure!(
            node_index == 0
                || !["GM", "MI", "RU", "PW", "PB"]
                    .iter()
                    .any(|key| node.contains_key(*key)),
            "SGF game metadata outside the root node is not supported"
        );
        setup(node, &mut self.board)?;
        if let Some(cv) = one(node, "CV")? {
            self.cube = cv.parse().context("SGF: invalid cube value")?;
            ensure!(
                self.cube > 0 && self.cube <= 32768 && (self.cube as u32).is_power_of_two(),
                "SGF: cube must be a power of two"
            );
        }
        let cp = one(node, "CP")?.filter(|v| matches!(*v, "c" | "w" | "b"));
        if let Some(co) = one(node, "CO")?.or(cp) {
            self.owner = match co {
                "c" => -1,
                "w" => 0,
                "b" => 1,
                "n" if self.context.crawford => -1,
                _ => bail!("Unsupported SGF cube owner {co}"),
            };
        }
        if native_setup_turn && ["AE", "AW", "AB"].iter().any(|key| node.contains_key(*key)) {
            self.expected_turn = None;
        } else if let Some(pl) = one(node, "PL")? {
            self.expected_turn = Some(match pl {
                "W" => 0,
                "B" => 1,
                _ => bail!("SGF: invalid PL"),
            });
        }
        Ok(())
    }

    fn apply_action(
        &mut self,
        game_index: usize,
        player: i32,
        value: &str,
        positions: &mut Vec<ImportedPosition>,
        source_moves: Option<&mut Vec<SourceMove>>,
    ) -> Result<()> {
        ensure!(!self.ended, "SGF contains moves after a cube drop");
        self.move_no += 1;
        let before = MoveSnapshot {
            board: self.board,
            pending: self.pending,
            cube: self.cube,
            owner: self.owner,
        };
        if matches!(value, "double" | "take" | "drop") {
            self.apply_cube_action(player, value)?;
        } else {
            self.apply_checker_action(game_index, player, value, positions)?;
        }
        if let Some(moves) = source_moves {
            self.record_source_move(game_index, player, value, &before, positions, moves)?;
        }
        Ok(())
    }

    fn apply_cube_action(&mut self, player: i32, value: &str) -> Result<()> {
        ensure!(
            self.context.cube_enabled,
            "SGF cube action in a game played without the cube"
        );
        self.context.decision_ids.push((
            ids::position_id(&self.board, player),
            self.match_id(self.cube, self.owner, player, [0, 0]),
        ));
        match value {
            "double" => {
                ensure!(
                    !self.context.crawford,
                    "SGF contains a double in a Crawford game"
                );
                ensure!(self.pending.is_none(), "Beavers/raccoons are not supported");
                ensure!(
                    self.owner == -1 || self.owner == player,
                    "SGF: player doubles an opponent-owned cube"
                );
                ensure!(
                    self.expected_turn.is_none_or(|turn| turn == player),
                    "SGF: wrong player doubles"
                );
                ensure!(self.cube < 32768, "SGF: cube exceeds GNUbg ID range");
                self.pending = Some(player);
            }
            "take" | "drop" => {
                let doubler = self
                    .pending
                    .take()
                    .context("SGF: cube response without a double")?;
                ensure!(
                    player != doubler,
                    "SGF: player responds to their own double"
                );
                if value == "take" {
                    self.cube *= 2;
                    self.owner = player;
                    self.expected_turn = Some(doubler);
                } else {
                    self.ended = true;
                    self.dropped_by = Some(player);
                    self.terminal_evidence = Some((self.board, self.cube, self.owner));
                }
            }
            _ => unreachable!("apply_cube_action called with non-cube value"),
        }
        Ok(())
    }

    fn apply_checker_action(
        &mut self,
        game_index: usize,
        player: i32,
        value: &str,
        positions: &mut Vec<ImportedPosition>,
    ) -> Result<()> {
        ensure!(
            self.pending.is_none(),
            "SGF: checker move before cube response"
        );
        ensure!(
            self.expected_turn.is_none_or(|turn| turn == player),
            "SGF: non-alternating checker moves"
        );
        let before = self.board;
        ensure!(
            before.iter().any(|n| *n > 0) && before.iter().any(|n| *n < 0),
            "SGF: checker move after a player has borne off every checker"
        );
        let (dice, notation) = play(&mut self.board, player, value)
            .with_context(|| format!("game {}, checker ply {}", game_index + 1, self.ply + 1))?;
        self.terminal_checker =
            !self.board.iter().any(|n| *n > 0) || !self.board.iter().any(|n| *n < 0);
        if self.terminal_checker {
            self.terminal_evidence = Some((self.board, self.cube, self.owner));
        }
        self.ply += 1;
        let position_id = ids::position_id(&before, player);
        let match_id = self.match_id(self.cube, self.owner, player, dice);
        self.context
            .decision_ids
            .push((position_id.clone(), match_id.clone()));
        positions.push(ImportedPosition {
            cube_enabled: self.context.cube_enabled,
            game_no: game_index as i32 + 1,
            ply: self.ply,
            player,
            dice,
            board: before.to_vec(),
            position_id,
            match_id,
            notation,
        });
        self.expected_turn = Some(1 - player);
        Ok(())
    }

    fn record_source_move(
        &self,
        game_index: usize,
        player: i32,
        value: &str,
        before: &MoveSnapshot,
        positions: &[ImportedPosition],
        moves: &mut Vec<SourceMove>,
    ) -> Result<()> {
        let checker = !matches!(value, "double" | "take" | "drop");
        let (dice, notation) = if checker {
            let position = positions.last().context("SGF: missing checker snapshot")?;
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
        let mut match_id = self.match_id(before.cube, before.owner, player, dice);
        if let Some(doubler) = before.pending {
            match_id = offered(&match_id, doubler)?;
        }
        let after_turn = if checker {
            1 - player
        } else {
            self.pending.or(before.pending).unwrap_or(player)
        };
        let mut after_match_id = self.match_id(self.cube, self.owner, after_turn, [0, 0]);
        if let Some(doubler) = self.pending {
            after_match_id = offered(&after_match_id, doubler)?;
        }
        moves.push(SourceMove {
            game_no: game_index as i32 + 1,
            move_no: self.move_no,
            kind: if checker { "checker" } else { "cube" },
            player,
            notation,
            checker_ply: checker.then_some(self.ply),
            board: before.board.to_vec(),
            dice,
            position_id: ids::position_id(&before.board, before.pending.unwrap_or(player)),
            match_id,
            after_board: self.board.to_vec(),
            after_match_id,
        });
        Ok(())
    }

    /// GNUbg writes MOVE_RESIGN only as the result's R suffix and restores
    /// it as a final source event after replaying the explicit move nodes.
    /// Preserve the existing checker and analysis-context import contracts.
    fn record_resignation(
        &self,
        root: &Node,
        game_index: usize,
        moves: &mut Vec<SourceMove>,
    ) -> Result<()> {
        if let Some((winner, points)) = one(root, "RE")?.and_then(|r| r.split_once('+'))
            && points.ends_with(['R', 'r'])
        {
            ensure!(
                self.pending.is_none() && !self.ended,
                "SGF: resignation after cube offer or drop"
            );
            let player = match winner {
                "W" => 1,
                "B" => 0,
                _ => bail!("SGF: invalid resignation winner"),
            };
            let match_id = self.match_id(self.cube, self.owner, player, [0, 0]);
            moves.push(SourceMove {
                game_no: game_index as i32 + 1,
                move_no: self.move_no + 1,
                kind: "resign",
                player,
                notation: "Resign".to_owned(),
                after_board: self.board.to_vec(),
                after_match_id: match_id.clone(),
                checker_ply: None,
                board: self.board.to_vec(),
                dice: [0, 0],
                position_id: ids::position_id(&self.board, player),
                match_id,
            });
        }
        Ok(())
    }

    fn finish(
        self,
        root: &Node,
        game_index: usize,
    ) -> Result<(crate::model::MatchGame, GameContext, i32, [i32; 2])> {
        let (result_board, result_cube, result_owner) = self
            .terminal_evidence
            .unwrap_or((self.board, self.cube, self.owner));
        let recorded = one(root, "RE")?;
        let outcome = crate::game_result::derive(
            &result_board,
            result_cube,
            self.context.jacoby && result_owner == -1,
            self.context.scores,
            self.terminal_checker,
            self.dropped_by,
            recorded,
        );
        let next_scores = next_scores_from_result(self.context.scores, recorded)?;
        let game = crate::model::MatchGame {
            cube_enabled: Some(self.context.cube_enabled),
            result: outcome,
            game_no: game_index as i32 + 1,
            position_count: self.ply,
            white_score: self.context.scores[0],
            black_score: self.context.scores[1],
            crawford: self.context.crawford,
        };
        Ok((game, self.context, result_cube, next_scores))
    }
}

fn next_scores_from_result(mut scores: [i32; 2], recorded: Option<&str>) -> Result<[i32; 2]> {
    if let Some(re) = recorded
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
            scores[side] = scores[side].checked_add(n).context("Score overflow")?;
        }
    }
    Ok(scores)
}

fn infer_results_from_following_scores(
    games: &mut [crate::model::MatchGame],
    trees: &[Vec<Node>],
    result_cubes: &[i32],
) {
    // Read only explicit next-root score fields. Inferred results must never
    // feed previous_scores or alter position/match IDs during source replay.
    for (index, (game, (next, &cube))) in games
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
