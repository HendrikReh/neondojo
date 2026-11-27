use super::container::{f32_at, f64_at, i32_at};
use crate::import::sgf::{coord::format_point, encode_retained_analysis};
use anyhow::{Result, ensure};
use serde_json::{Value, json};

fn level(value: i32) -> (String, Option<i32>, &'static str) {
    match value {
        0..=6 => (format!("{}-ply", value + 1), Some(value + 1), "evaluation"),
        12 => ("3-ply red".into(), Some(3), "evaluation"),
        100 => ("Rollout".into(), None, "rollout"),
        998 => ("Opening Book V2".into(), None, "opening_book"),
        999 => ("Opening Book V1".into(), None, "opening_book"),
        1000 => ("XGRoller".into(), None, "xgroller"),
        1001 => ("XGRoller+".into(), None, "xgroller"),
        1002 => ("XGRoller++".into(), None, "xgroller"),
        _ => (format!("Unknown (XG level {value})"), None, "unknown"),
    }
}

fn native_notation(endpoints: &[i32]) -> String {
    let point = |n: i32| format_point(if n < 0 { 0 } else { (n + 1) as usize });
    let moves: Vec<_> = endpoints
        .as_chunks::<2>()
        .0
        .iter()
        .take_while(|p| p[0] != -1)
        .map(|p| format!("{}/{}", point(p[0]), point(p[1])))
        .collect();
    if moves.is_empty() {
        "Cannot move".into()
    } else {
        moves.join(" ")
    }
}

pub(super) fn checker_analysis(record: &[u8], index: usize) -> Result<Option<Value>> {
    let count = i32_at(record, 120)?;
    ensure!(
        (0..=32).contains(&count),
        "XG: invalid analysis candidate count"
    );
    if count == 0 {
        return Ok(None);
    }
    let mut candidates = Vec::new();
    for i in 0..count as usize {
        let code = i16::from_le_bytes(record[1280 + i * 4..1282 + i * 4].try_into()?) as i32;
        let (depth, plies, method) = level(code);
        let endpoints: Vec<_> = record[1024 + i * 8..1032 + i * 8]
            .iter()
            .map(|b| i32::from(*b as i8))
            .collect();
        let values: Vec<_> = (0..7)
            .map(|j| f32_at(record, 1408 + i * 28 + j * 4))
            .collect::<Result<_>>()?;
        candidates.push(json!({
            "rank": i + 1,
            "notation": native_notation(&endpoints),
            "equity": values[6],
            "equity_kind": "cubeful",
            "probabilities": {
                "lose_backgammon": values[0],
                "lose_gammon": values[1],
                "lose": values[2],
                "win": values[3],
                "win_gammon": values[4],
                "win_backgammon": values[5]
            },
            "depth": depth,
            "depth_plies": plies,
            "method": method,
            "raw": {
                "level": code,
                "is_double_byte": record[1282 + i * 4],
                "endpoints": endpoints,
                "values": values,
                "rollout_index": i32_at(record, 2344 + i * 4)?
            }
        }));
    }
    let first = &candidates[0];
    Ok(Some(json!({
        "engine": "eXtreme Gammon",
        "engine_version": null,
        "depth": first["depth"],
        "depth_plies": first["depth_plies"],
        "method": first["method"],
        "candidates": candidates,
        "raw": {
            "record_index": index,
            "analyze_level": i32_at(record, 2472)?,
            "requested_level": i32_at(record, 160)?,
            "equity_loss": f64_at(record, 2312)?,
            "candidate_count": count
        }
    })))
}

pub(super) fn cube_analysis(record: &[u8], index: usize) -> Result<Option<Value>> {
    let code = i32_at(record, 232)?;
    if code < 0 {
        return Ok(None);
    }
    let (depth, plies, method) = level(code);
    let no_double: Vec<_> = (0..7)
        .map(|j| f32_at(record, 124 + j * 4))
        .collect::<Result<_>>()?;
    let double_take: Vec<_> = (0..7)
        .map(|j| f32_at(record, 168 + j * 4))
        .collect::<Result<_>>()?;
    let mut value = json!({
        "engine": "eXtreme Gammon",
        "engine_version": null,
        "depth": depth,
        "depth_plies": plies,
        "method": method,
        "candidates": [],
        "cube": {
            "equity_kind": "cubeful",
            "perspective": "doubler",
            "no_double": f32_at(record, 152)?,
            "double_take": f32_at(record, 156)?,
            "double_pass": f32_at(record, 160)?
        },
        "raw": {
            "record_index": index,
            "action": i32_at(record, 16)?,
            "analyze_level": code,
            "requested_level": i32_at(record, 256)?,
            "equity_loss": f64_at(record, 200)?,
            "take_equity_loss": f64_at(record, 216)?,
            "rollout_index": i32_at(record, 224)?,
            "no_double_values": no_double,
            "double_take_values": double_take
        }
    });
    let action = i32_at(record, 16)?;
    if action < 0 {
        value["played"] = json!(format!("Unknown (XG cube action {action})"));
    }
    Ok(Some(value))
}

pub(super) fn analysis_property(out: &mut String, key: &str, value: &Value) -> Result<()> {
    out.push_str(key);
    out.push('[');
    out.push_str(&encode_retained_analysis(value)?);
    out.push(']');
    Ok(())
}
