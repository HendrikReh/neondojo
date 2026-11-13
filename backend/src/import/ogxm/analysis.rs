//! Project supported OGXM decisions into the existing source-analysis catalog.
use super::wire::{Reader, Sections, has};
use super::{Node, Placement, steps_to_sgf};
use crate::import::sgf::encode_retained_analysis;
use crate::imported_analysis::{ImportedCandidate, probabilities, sgf_notation};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

#[derive(Clone, Default)]
struct Level<'a> {
    preset: Option<&'a str>,
    checker: Option<u32>,
    cube: Option<u32>,
    rollout: Option<bool>,
}

impl<'a> Level<'a> {
    fn read(&self, r: &mut Reader<'a>) -> Result<Self> {
        let (mask, mut r) = r.record()?;
        let mut level = self.clone();
        if has(mask, 0) {
            level.preset = Some(r.string()?);
        }
        if has(mask, 1) {
            level.checker = Some(r.var()?);
        }
        if has(mask, 2) {
            level.cube = Some(r.var()?);
        }
        if has(mask, 3) {
            r.record()?;
            level.rollout = Some(true);
        }
        if has(mask, 4) {
            level.rollout = Some(false);
        }
        r.finish(mask, 5)?;
        Ok(level)
    }

    fn method(&self) -> &'static str {
        if self.rollout == Some(true) {
            "rollout"
        } else if self.checker.is_some() || self.cube.is_some() {
            "evaluation"
        } else {
            "unknown"
        }
    }

    fn plies(&self, checker: bool) -> Option<i32> {
        if checker { self.checker } else { self.cube }.and_then(|n| n.try_into().ok())
    }

    fn label(&self, checker: bool) -> Option<String> {
        self.plies(checker)
            .map(|n| format!("{n}-ply"))
            .or_else(|| self.preset.map(str::to_owned))
    }
}

struct Block<'a> {
    id: uuid::Uuid,
    engine: &'static str,
    build: Option<&'a str>,
    model: Option<&'a str>,
    model_id: Option<&'a str>,
    met: Option<&'a str>,
    currency: Option<u32>,
    level: Level<'a>,
    source_count: u32,
}

impl<'a> Block<'a> {
    fn read(payload: &'a [u8], digest: &[u8; 32]) -> Result<Self> {
        let mut r = Reader::new(payload);
        let (mask, mut body) = r.record()?;
        r.finish(0, 0)?;
        let id = uuid::Uuid::from_bytes(body.array()?);
        if has(mask, 0) {
            ensure!(
                &body.array::<32>()? == digest,
                "OGXM analysis match digest mismatch"
            );
        }
        let engine = if has(mask, 1) {
            match body.var()? {
                0 => "HedgeHog",
                1 => "eXtreme Gammon",
                2 => "External engine",
                3 => "BGBlitz",
                4 => "Human annotation",
                _ => "Unknown",
            }
        } else {
            "Unknown"
        };
        let mut level = Level::default();
        if has(mask, 2) {
            level = level.read(&mut body)?;
        }
        if has(mask, 4) {
            let n = body.var()?;
            ensure!(n <= 100_000, "OGXM analysis coverage too large");
            for _ in 0..n {
                body.var()?;
            }
        }
        let model_id = has(mask, 5).then(|| body.string()).transpose()?;
        let model = has(mask, 6).then(|| body.string()).transpose()?;
        if has(mask, 7) {
            body.take(32)?;
        }
        let build = has(mask, 8).then(|| body.string()).transpose()?;
        let currency = has(mask, 9).then(|| body.var()).transpose()?;
        if has(mask, 10) {
            probability(&mut body)?;
        }
        let met = has(mask, 11).then(|| body.string()).transpose()?;
        if has(mask, 12) {
            body.string()?;
        }
        if has(mask, 13) {
            body.record()?;
        }
        for bit in [14, 15] {
            if has(mask, bit) {
                body.leb(10)?;
            }
        }
        if has(mask, 16) {
            body.var()?;
        }
        let source_count = if has(mask, 17) {
            let n = body.var()?;
            ensure!(n <= 64, "Too many OGXM analysis sources");
            body.take(n as usize * 16)?;
            n
        } else {
            0
        };
        if has(mask, 18) {
            body.record()?;
        }
        body.finish(mask, 19)?;
        Ok(Self {
            id,
            engine,
            build,
            model,
            model_id,
            met,
            currency,
            level,
            source_count,
        })
    }

    fn source(&self, body: &mut Reader<'_>) -> Result<u32> {
        let source = body.var()?;
        ensure!(
            source < self.source_count,
            "Invalid OGXM analysis source index"
        );
        Ok(source)
    }

    fn record(&self, level: &Level<'_>, checker: bool) -> Value {
        json!({
            "engine":self.engine, "engine_version":self.build,
            "method":level.method(), "depth":level.label(checker),
            "depth_plies":level.plies(checker), "analysis_id":self.id,
            "model_name":self.model, "model_id":self.model_id, "met_id":self.met,
        })
    }
}

fn kind(currency: Option<u32>) -> &'static str {
    match currency {
        Some(0) => "cubeless",
        Some(1) => "cubeful",
        Some(2 | 3) => "mwc",
        _ => "unknown",
    }
}

fn equity(r: &mut Reader<'_>, currency: Option<u32>) -> Result<f64> {
    let v = f64::from(i32::from_le_bytes(r.array()?)) / 1_000_000.0;
    ensure!(
        kind(currency) != "mwc" || (0.0..=1.0).contains(&v),
        "Invalid OGXM match winning chance"
    );
    Ok(v)
}

fn loss(r: &mut Reader<'_>) -> Result<f64> {
    Ok(f64::from(r.u32()?) / 1_000_000.0)
}

fn probability(r: &mut Reader<'_>) -> Result<f64> {
    let v = u16::from_le_bytes(r.array()?);
    ensure!(v <= 10000, "Invalid OGXM probability");
    Ok(f64::from(v) / 10_000.0)
}

fn probs(r: &mut Reader<'_>) -> Result<Value> {
    let mut values = [None; 5];
    for value in &mut values {
        *value = Some(probability(r)?);
    }
    Ok(probabilities(&values))
}

struct Alternative<'a> {
    steps: String,
    equity: f64,
    probabilities: Value,
    level: Option<Level<'a>>,
    played: bool,
    rollout_se: Option<f64>,
    cubeless_equity: Option<f64>,
}

fn checker<'a>(
    mask: u64,
    mut body: Reader<'a>,
    block: &Block<'a>,
    p: Placement,
    board: &[i32],
) -> Result<Value> {
    ensure!(mask != 0, "Empty OGXM checker decision");
    let dice = p
        .dice
        .context("OGXM checker analysis without a played roll")?;
    let mut alternatives = Vec::new();
    if has(mask, 0) {
        let n = body.var()?;
        ensure!((1..=1024).contains(&n), "Invalid OGXM alternative count");
        for _ in 0..n {
            let (bits, mut alt) = body.record()?;
            let count = alt.var()?;
            ensure!(count <= 4, "Invalid OGXM alternative steps");
            let bytes = alt.take(count as usize)?;
            let steps = match steps_to_sgf(bytes, p.seat, dice, false) {
                Ok(steps) => steps,
                Err(_) => {
                    // HedgeHog analysis exports can store the distance to off
                    // rather than the oversized die. Validate this fallback
                    // against the actual roll and the replayed position.
                    let steps = steps_to_sgf(bytes, p.seat, dice, true)?;
                    let mut board: [i32; 26] =
                        board.try_into().context("Invalid OGXM position board")?;
                    let [a, b] = dice;
                    crate::import::sgf::play(&mut board, p.seat as i32, &format!("{a}{b}{steps}"))
                        .context("Invalid OGXM checker alternative")?;
                    steps
                }
            };
            let value = equity(&mut alt, block.currency)?;
            let probabilities = if has(bits, 0) {
                probs(&mut alt)?
            } else {
                Value::Null
            };
            let level = has(bits, 1)
                .then(|| Level::default().read(&mut alt))
                .transpose()?;
            let rollout_se = has(bits, 3).then(|| loss(&mut alt)).transpose()?;
            let cubeless = has(bits, 4)
                .then(|| equity(&mut alt, block.currency.filter(|v| *v == 2)))
                .transpose()?;
            alt.finish(bits, 5)?;
            alternatives.push(Alternative {
                steps,
                equity: value,
                probabilities,
                level,
                played: has(bits, 2),
                rollout_se,
                cubeless_equity: cubeless,
            });
        }
    }
    let total = has(mask, 1).then(|| body.var()).transpose()?;
    let best = has(mask, 2)
        .then(|| equity(&mut body, block.currency))
        .transpose()?;
    let error = has(mask, 3).then(|| loss(&mut body)).transpose()?;
    let level = if has(mask, 4) {
        block.level.read(&mut body)?
    } else {
        block.level.clone()
    };
    let rolled = has(mask, 5).then(|| body.var()).transpose()?;
    let deep = has(mask, 6).then(|| body.var()).transpose()?;
    let tags = has(mask, 7).then(|| body.var()).transpose()?;
    let source = has(mask, 8).then(|| block.source(&mut body)).transpose()?;
    let band = has(mask, 9).then(|| body.var()).transpose()?;
    body.finish(mask, 10)?;

    let mut record = block.record(&level, true);
    record["currency"] = json!(block.currency);
    record["best_equity"] = json!(best.or_else(|| alternatives.first().map(|alt| alt.equity)));
    record["equity_loss"] = json!(error.or_else(|| {
        let best = alternatives.first()?.equity;
        let played = alternatives.iter().find(|alt| alt.played)?.equity;
        Some((best - played).max(0.0))
    }));
    record["alternatives_total"] = json!(total);
    record["rollouts_done"] = json!(rolled);
    record["deep_searched"] = json!(deep);
    record["position_tags"] = json!(tags);
    record["producer_ref"] = json!(source);
    record["source_band"] = json!(band);
    let candidates: Vec<_> = alternatives
        .into_iter()
        .enumerate()
        .map(|(index, alt)| {
            let effective = match alt.level {
                Some(own) => Level {
                    preset: own.preset.or(level.preset),
                    checker: own.checker.or(level.checker),
                    cube: own.cube.or(level.cube),
                    rollout: own.rollout.or(level.rollout),
                },
                None => level.clone(),
            };
            ImportedCandidate {
                rank: index as i32 + 1,
                notation: sgf_notation(&alt.steps, p.seat as i32),
                equity: Some(alt.equity),
                equity_kind: kind(block.currency).into(),
                probabilities: alt.probabilities,
                depth: effective.label(true),
                depth_plies: effective.plies(true),
                method: Some(effective.method().into()),
                raw: json!({
                    "is_played": alt.played,
                    "rollout_se": alt.rollout_se,
                    "cubeless_equity": alt.cubeless_equity,
                }),
            }
        })
        .collect();
    record["candidates"] = serde_json::to_value(candidates)?;
    Ok(record)
}

fn cube<'a>(mask: u64, mut body: Reader<'a>, block: &Block<'a>, p: Placement) -> Result<Value> {
    let response = matches!(p.action, 22 | 23);
    let verdict = match body.var()? {
        0 if !response => "No double",
        1 if !response => "Double",
        2 if response => "Take",
        3 if response => "Pass",
        4 if !response => "Too good",
        5 | 6 if response => "Unsupported response",
        _ => "Unknown",
    };
    let mut values = [None; 3];
    for (bit, value) in values.iter_mut().enumerate() {
        if has(mask, bit as u32) {
            *value = Some(i32::from_le_bytes(body.array()?));
        }
    }
    let probabilities = has(mask, 3).then(|| probs(&mut body)).transpose()?;
    let error = has(mask, 4).then(|| loss(&mut body)).transpose()?;
    let take_point = has(mask, 5).then(|| probability(&mut body)).transpose()?;
    let level = if has(mask, 7) {
        block.level.read(&mut body)?
    } else {
        block.level.clone()
    };
    let take_value = has(mask, 10).then(|| body.array::<4>()).transpose()?;
    let currency = has(mask, 11)
        .then(|| body.var())
        .transpose()?
        .or(block.currency);
    let source = has(mask, 12).then(|| block.source(&mut body)).transpose()?;
    body.finish(mask, 13)?;
    let [nd, dt, dp] = values.map(|v| v.map(|v| f64::from(v) / 1_000_000.0));
    ensure!(
        kind(currency) != "mwc"
            || [nd, dt, dp]
                .into_iter()
                .flatten()
                .all(|v| (0.0..=1.0).contains(&v)),
        "Invalid OGXM cube match winning chance"
    );
    let mut record = block.record(&level, false);
    record["producer_ref"] = json!(source);
    record["cube"] = json!({
        "verdict":verdict, "equity_kind":kind(currency), "currency":currency,
        "no_double":nd, "double_take":dt, "double_pass":dp,
        "probabilities":probabilities, "equity_loss":error, "take_point":take_point,
        "cubeful_take_value":take_value.map(|v| f64::from(i32::from_le_bytes(v)) / 1_000_000.0),
        "is_optional":has(mask,8), "is_free_cube":has(mask,9),
        "window_searched":has(mask,6),
        "equity_player":if response {1-p.seat} else {p.seat},
    });
    Ok(record)
}

pub(super) fn attach(
    sections: &Sections<'_>,
    placements: &[Placement],
    positions: &[crate::import::ImportedPosition],
    trees: &mut [Vec<Node>],
) -> Result<()> {
    let digest = sections.match_digest();
    let mut ids = std::collections::HashSet::new();
    let mut retained_bytes = 0;
    for (header, decisions) in &sections.analyses {
        let block = Block::read(header, &digest)?;
        ensure!(ids.insert(block.id), "Duplicate OGXM analysis id");
        let mut r = Reader::new(decisions);
        let mut previous = None;
        let mut count = 0;
        while !r.remaining().is_empty() {
            count += 1;
            ensure!(count <= 300_000, "Too many OGXM decisions");
            let (mask, mut body) = r.record()?;
            let ply = body.var()? as usize;
            let kind = body.var()?;
            ensure!(
                previous.is_none_or(|last| last < (ply, kind)),
                "Unordered or duplicate OGXM decision"
            );
            previous = Some((ply, kind));
            let p = *placements
                .get(ply)
                .context("OGXM decision references an absent ply")?;
            let (key, record) = match kind {
                0 => {
                    let position = p
                        .checker_index
                        .and_then(|index| positions.get(index))
                        .context("OGXM checker analysis without a played position")?;
                    (
                        "OGA",
                        checker(mask, body, &block, p, &position.board).with_context(|| {
                            format!(
                                "OGXM game {}, checker ply {}",
                                position.game_no, position.ply
                            )
                        })?,
                    )
                }
                1 => {
                    ensure!(
                        matches!(p.action, 0..=23),
                        "OGXM cube decision attached to an incompatible ply"
                    );
                    if p.node.is_none() {
                        continue;
                    }
                    ("OGC", cube(mask, body, &block, p)?)
                }
                _ => continue, // Raw source retains roll, resignation and future decision kinds.
            };
            let node = p.node.context("OGXM decision lacks a played node")?;
            let encoded = encode_retained_analysis(&record)?;
            retained_bytes += encoded.len();
            ensure!(
                retained_bytes <= 24 * 1024 * 1024,
                "OGXM retained analysis exceeds 24 MiB"
            );
            trees[p.game][node]
                .entry(key.into())
                .or_default()
                .push(encoded);
        }
    }
    Ok(())
}
