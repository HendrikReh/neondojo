use super::{ImportedMatch, ImportedPosition, ids};
use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;

pub(crate) type Node = BTreeMap<String, Vec<String>>;
const MAX_SGF_BYTES: usize = 32 * 1024 * 1024;
const MAX_NODES: usize = 100_000;

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    nodes: usize,
}
impl Parser<'_> {
    fn ws(&mut self) {
        while self
            .bytes
            .get(self.pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.pos += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.bytes.get(self.pos).copied()
    }
    fn expect(&mut self, c: u8) -> Result<()> {
        ensure!(
            self.peek() == Some(c),
            "SGF: expected '{}' at byte {}",
            c as char,
            self.pos
        );
        self.pos += 1;
        Ok(())
    }
    fn value(&mut self) -> Result<String> {
        self.expect(b'[')?;
        let mut out = Vec::new();
        loop {
            let c = *self
                .bytes
                .get(self.pos)
                .context("SGF: unterminated property value")?;
            self.pos += 1;
            match c {
                b']' => return String::from_utf8(out).context("SGF: invalid UTF-8 property"),
                b'\\' => {
                    let next = *self.bytes.get(self.pos).context("SGF: incomplete escape")?;
                    self.pos += 1;
                    if next == b'\r' || next == b'\n' {
                        if self
                            .bytes
                            .get(self.pos)
                            .is_some_and(|b| (*b == b'\r' || *b == b'\n') && *b != next)
                        {
                            self.pos += 1;
                        }
                    } else {
                        out.push(next);
                    }
                }
                0 => bail!("SGF: NUL in property"),
                _ => out.push(c),
            }
            ensure!(
                out.len() <= 1024 * 1024,
                "SGF: property exceeds 1 MiB limit"
            );
        }
    }
    fn tree(&mut self, depth: usize) -> Result<Vec<Node>> {
        ensure!(depth < 64, "SGF: nesting limit exceeded");
        self.expect(b'(')?;
        let mut sequence = Vec::new();
        while self.peek() == Some(b';') {
            self.pos += 1;
            self.nodes += 1;
            ensure!(self.nodes <= MAX_NODES, "SGF: node limit exceeded");
            let mut node = Node::new();
            while self.peek().is_some_and(|c| c.is_ascii_uppercase()) {
                let start = self.pos;
                while self.bytes.get(self.pos).is_some_and(u8::is_ascii_uppercase) {
                    self.pos += 1;
                }
                let key = std::str::from_utf8(&self.bytes[start..self.pos])?.to_owned();
                let mut values = Vec::new();
                while self.peek() == Some(b'[') {
                    values.push(self.value()?);
                }
                ensure!(!values.is_empty(), "SGF: property {key} has no value");
                ensure!(
                    node.insert(key.clone(), values).is_none(),
                    "SGF: duplicate property {key}"
                );
            }
            sequence.push(node);
        }
        ensure!(!sequence.is_empty(), "SGF: empty game tree");
        if self.peek() == Some(b'(') {
            sequence.extend(self.tree(depth + 1)?);
            ensure!(
                self.peek() != Some(b'('),
                "SGF variations are ambiguous; export the played main line before importing"
            );
        }
        self.expect(b')')?;
        Ok(sequence)
    }
}

fn one<'a>(node: &'a Node, name: &str) -> Result<Option<&'a str>> {
    let Some(values) = node.get(name) else {
        return Ok(None);
    };
    ensure!(values.len() == 1, "SGF: {name} requires exactly one value");
    Ok(Some(&values[0]))
}
fn number(value: &str, name: &str) -> Result<i32> {
    let n: i32 = value
        .trim()
        .parse()
        .with_context(|| format!("SGF: invalid {name}: {value}"))?;
    ensure!(
        (0..=32767).contains(&n),
        "SGF: {name} exceeds GNUbg ID range"
    );
    Ok(n)
}

pub(crate) fn initial_board() -> [i32; 26] {
    let mut b = [0; 26];
    for (p, n) in [
        (6, 5),
        (8, 3),
        (13, 5),
        (24, 2),
        (1, -2),
        (12, -5),
        (17, -3),
        (19, -5),
    ] {
        b[p] = n;
    }
    b
}
fn valid_board(b: &[i32; 26]) -> Result<()> {
    ensure!(b[0] <= 0 && b[25] >= 0, "SGF: invalid bar ownership");
    ensure!(
        b.iter().map(|x| x.max(&0)).sum::<i32>() <= 15,
        "SGF: more than 15 white checkers"
    );
    ensure!(
        b.iter().map(|x| (-x).max(0)).sum::<i32>() <= 15,
        "SGF: more than 15 black checkers"
    );
    Ok(())
}
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
fn setup(node: &Node, board: &mut [i32; 26]) -> Result<()> {
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
    valid_board(board)
}
fn local_board(board: &[i32; 26], player: i32) -> [i32; 26] {
    if player == 0 {
        *board
    } else {
        std::array::from_fn(|i| -board[25 - i])
    }
}
fn step(b: &[i32; 26], from: usize, to: usize, die: i32) -> Option<[i32; 26]> {
    if !(1..=25).contains(&from) || to >= from || b[from] <= 0 || (b[25] > 0 && from != 25) {
        return None;
    }
    if to > 0 {
        if from - to != die as usize || b[to] < -1 {
            return None;
        }
    } else {
        if from > 6 || b[7..=25].iter().any(|n| *n > 0) {
            return None;
        }
        if die < from as i32 || (die > from as i32 && b[from + 1..=6].iter().any(|n| *n > 0)) {
            return None;
        }
    }
    let mut out = *b;
    out[from] -= 1;
    if to > 0 {
        if out[to] == -1 {
            out[to] = 0;
            out[0] -= 1;
        }
        out[to] += 1;
    }
    Some(out)
}
fn any_step(board: &[i32; 26], die: i32) -> bool {
    (1..=25).any(|from| step(board, from, from.saturating_sub(die as usize), die).is_some())
}
fn max_plays(board: &[i32; 26], dice: &[i32]) -> usize {
    let mut best = 0;
    for (i, die) in dice.iter().enumerate() {
        if i > 0 && dice[i - 1] == *die {
            continue;
        }
        let mut rest = dice.to_vec();
        rest.remove(i);
        for from in 1usize..=25 {
            if let Some(after) = step(board, from, from.saturating_sub(*die as usize), *die) {
                best = best.max(1 + max_plays(&after, &rest));
                if best == dice.len() {
                    return best;
                }
            }
        }
    }
    best
}
fn apply_pairs(board: &[i32; 26], moves: &[(usize, usize)], dice: &[i32]) -> Option<[i32; 26]> {
    let Some(&(from, to)) = moves.first() else {
        // Refuse missing checker moves when a remaining die is still playable.
        return (!dice.iter().any(|d| any_step(board, *d))).then_some(*board);
    };
    for (i, die) in dice.iter().enumerate() {
        if i > 0 && dice[i - 1] == *die {
            continue;
        }
        if let Some(next) = step(board, from, to, *die) {
            let mut rest = dice.to_vec();
            rest.remove(i);
            if let Some(result) = apply_pairs(&next, &moves[1..], &rest) {
                return Some(result);
            }
        }
    }
    None
}

type ExpandedPlay = ([i32; 26], Vec<(usize, usize)>);

fn expand_runs(
    board: &[i32; 26],
    runs: &[(usize, usize)],
    dice: &[i32],
    remaining: usize,
) -> Option<ExpandedPlay> {
    if runs.is_empty() {
        return (remaining == 0).then(|| (*board, Vec::new()));
    }
    if remaining == 0 || runs.len() > remaining || remaining > dice.len() {
        return None;
    }
    for (run, &(from, target)) in runs.iter().enumerate() {
        if runs[..run].contains(&(from, target)) {
            continue;
        }
        for (die_index, &die) in dice.iter().enumerate() {
            if die_index > 0 && dice[die_index - 1] == die {
                continue;
            }
            let to = from.saturating_sub(die as usize);
            // Joined notation has no intermediate hits. A hit must be an explicit
            // endpoint in the source, otherwise choosing a path would invent one.
            if to < target || (to != target && board[to] == -1) {
                continue;
            }
            let Some(next) = step(board, from, to, die) else {
                continue;
            };
            let mut next_runs = runs.to_vec();
            if to == target {
                next_runs.remove(run);
            } else {
                next_runs[run].0 = to;
            }
            let mut next_dice = dice.to_vec();
            next_dice.remove(die_index);
            if let Some((after, mut steps)) =
                expand_runs(&next, &next_runs, &next_dice, remaining - 1)
            {
                steps.insert(0, (from, to));
                return Some((after, steps));
            }
        }
    }
    None
}

/// XG files imported from text can join several dice into one From/To run.
/// Expand at most four dice using the replayed board, including bar priority and
/// legal overshoots. SGF imports themselves continue to require explicit steps.
pub(crate) fn expand_xg_play(
    board: &mut [i32; 26],
    player: i32,
    dice: [i32; 2],
    runs: &[(usize, usize)],
) -> Result<Vec<(usize, usize)>> {
    ensure!(dice.iter().all(|d| (1..=6).contains(d)), "XG: invalid dice");
    ensure!(
        runs.len() <= 4
            && runs
                .iter()
                .all(|&(from, to)| (1..=25).contains(&from) && to < from),
        "XG: invalid checker run"
    );
    let available = if dice[0] == dice[1] {
        vec![dice[0]; 4]
    } else {
        dice.to_vec()
    };
    let before = local_board(board, player);
    let required = max_plays(&before, &available);
    let (after, steps) = expand_runs(&before, runs, &available, required)
        .context("XG: checker runs cannot be expanded to a legal, complete play")?;
    if steps.len() == 1 && dice[0] != dice[1] {
        let high = dice[0].max(dice[1]);
        let (from, to) = steps[0];
        ensure!(
            step(&before, from, to, high).is_some() || !any_step(&before, high),
            "XG: single checker move must use the larger die"
        );
    }
    *board = local_board(&after, player);
    Ok(steps)
}

/// Validate and apply one SGF checker play. Coordinates in SGF are absolute;
/// notation is always from the moving player's perspective.
fn play(board: &mut [i32; 26], player: i32, value: &str) -> Result<([i32; 2], String)> {
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
    let available = if dice[0] == dice[1] {
        vec![dice[0]; 4]
    } else {
        dice.to_vec()
    };
    let before = local_board(board, player);
    ensure!(
        pairs.len() == max_plays(&before, &available),
        "SGF: checker play does not use the maximum possible dice"
    );
    let after = apply_pairs(&before, &pairs, &available)
        .with_context(|| format!("SGF: illegal or incomplete checker move {value}"))?;
    if pairs.len() == 1 && dice[0] != dice[1] {
        let high = dice[0].max(dice[1]);
        let (from, to) = pairs[0];
        ensure!(
            step(&before, from, to, high).is_some() || !any_step(&before, high),
            "SGF: single checker move must use the larger die"
        );
    }
    *board = local_board(&after, player);
    Ok((
        dice,
        if notation.is_empty() {
            "Cannot move".to_owned()
        } else {
            notation.join(" ")
        },
    ))
}

pub(crate) fn parse_trees(input: &str) -> Result<Vec<Vec<Node>>> {
    ensure!(input.len() <= MAX_SGF_BYTES, "SGF exceeds 32 MiB limit");
    let input = input.trim_start_matches('\u{feff}');
    let mut parser = Parser {
        bytes: input.as_bytes(),
        pos: 0,
        nodes: 0,
    };
    let mut trees = Vec::new();
    while parser.peek().is_some() {
        trees.push(parser.tree(0)?);
    }
    ensure!(!trees.is_empty(), "SGF contains no games");
    Ok(trees)
}

/// Validated state for every game, including games without checker plays.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GameContext {
    pub length: i32,
    pub scores: [i32; 2],
    pub crawford: bool,
    pub jacoby: bool,
    pub decision_ids: Vec<(String, String)>,
}

pub fn parse_sgf(input: &str) -> Result<ImportedMatch> {
    let (result, _) = parse_sgf_with_context(input)?;
    ensure!(
        !result.positions.is_empty(),
        "SGF contains no checker-play decisions"
    );
    Ok(result)
}

/// Internal analysis accepts an individual empty or cube-only game exported
/// from an otherwise importable match, while public imports still need checkers.
pub(crate) fn parse_sgf_with_context(input: &str) -> Result<(ImportedMatch, Vec<GameContext>)> {
    parse_sgf_context(input, false)
}

/// GNUbg 1.05.002 can write PL[W] for a Black setup because its SETBOARD record
/// lacks fPlayer. Only native output may infer turn from the next actual move;
/// callers must compare all resulting context IDs against the strict source.
pub(crate) fn parse_native_sgf_with_context(
    input: &str,
) -> Result<(ImportedMatch, Vec<GameContext>)> {
    parse_sgf_context(input, true)
}

fn parse_sgf_context(
    input: &str,
    native_setup_turn: bool,
) -> Result<(ImportedMatch, Vec<GameContext>)> {
    let trees = parse_trees(input)?;
    let mut contexts = Vec::new();
    let mut result = ImportedMatch {
        players: ["White".into(), "Black".into()],
        match_length: 0,
        positions: Vec::new(),
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
        if let Some(rules) = one(root, "RU")? {
            for rule in rules.split(':') {
                match rule.trim() {
                    "" | "Crawford" | "Backgammon" | "Standard" => {}
                    "CrawfordGame" => crawford = true,
                    "Jacoby" => jacoby = true,
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
            decision_ids: Vec::new(),
        };
        let mut board = initial_board();
        let mut cube = 1i32;
        let mut owner = -1;
        let mut pending = None;
        let mut expected_turn = None;
        let mut ended = false;
        let mut ply = 0;
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
            if matches!(value, "double" | "take" | "drop") {
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
                    ply += 1;
                    context.decision_ids.push((
                        ids::position_id(&before, player),
                        ids::match_id(length, scores, cube, owner, crawford, jacoby, player, dice),
                    ));
                    result.positions.push(ImportedPosition {
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
        }
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
    Ok((result, contexts))
}
