//! Immutable analysis supplied by the imported artifact, separate from engine jobs.
pub(crate) mod sage;
use crate::import::sgf::{Node, SourceMove, parse_sgf_source_moves, parse_trees};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::json;
use utoipa::{OpenApi, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportedCandidate {
    pub rank: i32,
    pub notation: String,
    pub equity: Option<f64>,
    pub equity_kind: String,
    pub probabilities: Value,
    pub depth: Option<String>,
    #[serde(default)]
    pub depth_plies: Option<i32>,
    #[serde(default)]
    pub method: Option<String>,
    pub raw: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportedAnalysisData {
    pub game_no: i32,
    pub source_move_no: i32,
    pub checker_ply: Option<i32>,
    #[serde(default)]
    pub position_id: Option<String>,
    #[serde(default)]
    pub gnubg_match_id: Option<String>,
    #[serde(default)]
    pub dice: Option<[i32; 2]>,
    pub kind: String,
    pub player: i32,
    pub played: String,
    pub engine: String,
    pub engine_version: Option<String>,
    pub method: String,
    pub depth: Option<String>,
    pub depth_plies: Option<i32>,
    pub candidates: Vec<ImportedCandidate>,
    pub cube: Option<Value>,
    pub raw: Value,
}
fn text(node: &Node, key: &str) -> Option<String> {
    node.get(key)?.first().cloned()
}
fn native_number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|n| n.is_finite())
        .or_else(|| value.as_str().and_then(|s| finite(s.trim())))
}
fn finite(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().filter(|n| n.is_finite())
}
pub(crate) fn sgf_notation(value: &str, player: i32) -> String {
    let point = |c: u8| match c {
        b'y' => "bar".into(),
        b'z' => "off".into(),
        b'a'..=b'x' => {
            if player == 0 {
                (i32::from(c - b'a') + 1).to_string()
            } else {
                (i32::from(b'x' - c) + 1).to_string()
            }
        }
        _ => "?".into(),
    };
    let moves: Vec<_> = value
        .as_bytes()
        .chunks_exact(2)
        .map(|p| format!("{}/{}", point(p[0]), point(p[1])))
        .collect();
    if moves.is_empty() {
        "Cannot move".into()
    } else {
        moves.join(" ")
    }
}
fn gnubg_depth(token: Option<&&str>) -> (Option<String>, Option<i32>) {
    let Some(token) = token else {
        return (None, None);
    };
    let plies = token
        .trim_end_matches(['C', 'c'])
        .parse::<i32>()
        .ok()
        .filter(|n| *n >= 0 && *n <= 64);
    (
        plies.map(|n| {
            format!(
                "{n}-ply{}",
                if token.ends_with('C') {
                    " cubeful"
                } else {
                    " cubeless"
                }
            )
        }),
        plies,
    )
}
fn keyword_number(tokens: &[&str], keyword: &str) -> Option<f64> {
    tokens
        .iter()
        .position(|s| *s == keyword)
        .and_then(|i| tokens.get(i + 1))
        .and_then(|s| finite(s))
}
fn rollout_depth(tokens: &[&str]) -> Option<String> {
    let trials = keyword_number(tokens, "Trials");
    Some(
        trials
            .map(|n| format!("Rollout ({n:.0} trials)"))
            .unwrap_or_else(|| "Rollout".into()),
    )
}
fn rollout_output(tokens: &[&str], section: Option<&str>) -> Vec<Option<f64>> {
    let start = match section {
        Some(label) => match tokens.iter().position(|s| *s == label) {
            Some(start) => start + 1,
            None => return vec![None; 7],
        },
        None => 0,
    };
    let end = if section.is_some() {
        tokens[start..]
            .iter()
            .position(|s| matches!(*s, "NoDouble" | "DoubleTake"))
            .map(|i| start + i)
            .unwrap_or(tokens.len())
    } else {
        tokens.len()
    };
    let section = &tokens[start..end];
    let output = section.iter().position(|s| *s == "Output").map(|n| n + 1);
    (0..7)
        .map(|n| {
            output
                .and_then(|i| section.get(i + n))
                .and_then(|s| finite(s))
        })
        .collect()
}
pub(crate) fn probabilities(values: &[Option<f64>]) -> Value {
    json!({"win":values.first().copied().flatten(),"win_gammon":values.get(1).copied().flatten(),"win_backgammon":values.get(2).copied().flatten(),"lose_gammon":values.get(3).copied().flatten(),"lose_backgammon":values.get(4).copied().flatten()})
}
fn base(m: &SourceMove, kind: &str, engine: &str, version: Option<String>) -> ImportedAnalysisData {
    let pre_roll = kind == "cube" && m.kind == "checker";
    let mut match_id = m.match_id.clone();
    if pre_roll {
        use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
        if let Ok(mut bytes) = STANDARD_NO_PAD.decode(&match_id) {
            for bit in 15..21 {
                bytes[bit / 8] &= !(1 << (bit % 8));
            }
            match_id = STANDARD_NO_PAD.encode(bytes);
        }
    }
    ImportedAnalysisData {
        position_id: Some(m.position_id.clone()),
        gnubg_match_id: Some(match_id),
        dice: Some(if pre_roll { [0, 0] } else { m.dice }),
        game_no: m.game_no,
        source_move_no: m.move_no,
        checker_ply: m.checker_ply,
        kind: kind.into(),
        player: m.player,
        played: if kind == "cube" && m.kind == "checker" {
            "No double".into()
        } else {
            m.notation.clone()
        },
        engine: engine.into(),
        engine_version: version,
        method: "unknown".into(),
        depth: None,
        depth_plies: None,
        candidates: Vec::new(),
        cube: None,
        raw: Value::Null,
    }
}
fn foreign(m: &SourceMove, kind: &str, value: Value) -> Result<ImportedAnalysisData> {
    let mut record = base(
        m,
        kind,
        value["engine"].as_str().unwrap_or("Unknown"),
        value["engine_version"].as_str().map(str::to_owned),
    );
    record.method = value["method"].as_str().unwrap_or("unknown").into();
    record.depth = value["depth"].as_str().map(str::to_owned);
    record.depth_plies = value["depth_plies"]
        .as_i64()
        .and_then(|n| i32::try_from(n).ok());
    record.candidates =
        serde_json::from_value(value.get("candidates").cloned().unwrap_or(json!([])))?;
    record.cube = value.get("cube").cloned();
    if let Some(played) = value["played"].as_str() {
        record.played = played.into();
    }
    record.raw = value;
    Ok(record)
}
fn bgf_analysis(m: &SourceMove, value: Value) -> Vec<ImportedAnalysisData> {
    let has_checker = value["moveAnalysis"]
        .as_array()
        .is_some_and(|a| !a.is_empty())
        || value["dancingEquity"].is_object();
    let has_cube = value["equity"].is_object() || value["cubeAnalysis"].is_object();
    if !has_checker && !has_cube && !value["pr"].is_object() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let engine = value["engine"]
        .as_str()
        .unwrap_or("Unknown (BGBlitz source)");
    let mut record = base(
        m,
        if has_checker {
            "checker"
        } else if m.kind == "cube" {
            "cube"
        } else {
            "source"
        },
        engine,
        None,
    );
    record.depth_plies = value["ply"]
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .filter(|n| *n >= 0);
    record.depth = record
        .depth_plies
        .map(|n| format!("{n}-ply (BGBlitz source)"));
    record.method = if record.depth_plies.is_some() {
        "evaluation"
    } else {
        "unknown"
    }
    .into();
    if let Some(candidates) = value["moveAnalysis"].as_array() {
        record.candidates=candidates.iter().enumerate().map(|(i,c)| {
            let mov=&c["move"];
            let mut parts=Vec::new();
            if let (Some(from),Some(to))=(mov["from"].as_array(),mov["to"].as_array()) {
                for (f,t) in from.iter().zip(to) {
                    let (Some(f),Some(t))=(f.as_i64(),t.as_i64())else{break};
                    if f<0{break} let point=|n:i64|if n==25{"bar".into()}else if n==0{"off".into()}else{n.to_string()};
                    parts.push(format!("{}/{}",point(f),point(t)));
                }
            }
            let plies=c["ply"].as_i64().and_then(|n|i32::try_from(n).ok()).filter(|n|*n>=0).or(record.depth_plies);
            let (equity,equity_kind)=match c["eq"]["useMatchEquity"].as_bool(){Some(true)=>(native_number(&c["eq"]["matchEquity"]),"mwc"),Some(false)=>(native_number(&c["eq"]["equity"]),"cubeless"),None=>(native_number(&c["eq"]["equity"]),"unknown")};
            ImportedCandidate {rank:i as i32+1,notation:parts.join(" "),equity,equity_kind:equity_kind.into(),probabilities:json!({"win":native_number(&c["eq"]["myWins"]),"win_gammon":native_number(&c["eq"]["myGammon"]),"win_backgammon":native_number(&c["eq"]["myBackGammon"]),"lose_gammon":native_number(&c["eq"]["oppGammon"]),"lose_backgammon":native_number(&c["eq"]["oppBackGammon"])}),depth:plies.map(|n|format!("{n}-ply (BGBlitz source)")),depth_plies:plies,method:Some(record.method.clone()),raw:c.clone()}
        }).collect();
    }
    if m.kind == "cube" {
        record.cube = Some(json!({"equity_kind":"unknown","source":value["equity"]}));
    }
    record.raw = value.clone();
    result.push(record);
    if has_cube && m.kind == "checker" && value["equity"]["cubeDecision"].is_object() {
        let mut cube = base(m, "cube", engine, None);
        cube.depth = result[0].depth.clone();
        cube.depth_plies = result[0].depth_plies;
        cube.cube = Some(json!({"equity_kind":"unknown","source":value["equity"]["cubeDecision"]}));
        cube.raw = value;
        result.push(cube);
    }
    result
}
/// Extract only analysis actually present in the validated source, with its own units.
pub(crate) fn extract(sgf: &str) -> Result<Vec<ImportedAnalysisData>> {
    let (imported, moves) = parse_sgf_source_moves(sgf)?;
    let trees = parse_trees(sgf)?;
    let mut result = Vec::new();
    for (game_index, nodes) in trees.iter().enumerate() {
        let application = text(&nodes[0], "AP").unwrap_or_default();
        let (engine, version) = if let Some(v) = application.strip_prefix("GNU Backgammon:") {
            ("GNU Backgammon", Some(v.to_owned()))
        } else if application == "GNU Backgammon" {
            ("GNU Backgammon", None)
        } else {
            ("Unknown", None)
        };
        let source_moves: Vec<_> = moves
            .iter()
            .filter(|m| m.game_no == game_index as i32 + 1 && m.kind != "resign")
            .collect();
        for (node, m) in nodes
            .iter()
            .filter(|n| n.contains_key("W") || n.contains_key("B"))
            .zip(source_moves)
        {
            if let Some(value) = text(node, "BGTSC") {
                result.push(sage::extract(m, &value)?);
            }
            for (property, kind) in [("XGA", "checker"), ("XGC", "cube")] {
                if let Some(v) = text(node, property) {
                    result.push(foreign(
                        m,
                        kind,
                        crate::import::sgf::decode_retained_analysis(&v)
                            .context("Invalid retained XG analysis")?,
                    )?);
                }
            }
            for (property, kind) in [("OGA", "checker"), ("OGC", "cube")] {
                for v in node.get(property).into_iter().flatten() {
                    result.push(foreign(
                        m,
                        kind,
                        crate::import::sgf::decode_retained_analysis(v)
                            .context("Invalid retained OGXM analysis")?,
                    )?);
                }
            }
            if let Some(v) = text(node, "BGA") {
                result.extend(bgf_analysis(
                    m,
                    crate::import::sgf::decode_retained_analysis(&v)
                        .context("Invalid retained BGF analysis")?,
                ));
            }
            if let Some(values) = node.get("A") {
                let mut record = base(m, "checker", engine, version.clone());
                for (i, value) in values.iter().skip(1).enumerate() {
                    let tokens: Vec<_> = value.split_whitespace().collect();
                    let recognized = tokens.get(1..4) == Some(&["E", "ver", "3"]);
                    let rollout = tokens.get(1..4) == Some(&["X", "ver", "3"]);
                    let (depth, plies) = if recognized {
                        gnubg_depth(tokens.get(10))
                    } else if rollout {
                        (rollout_depth(&tokens), None)
                    } else {
                        (None, None)
                    };
                    let equity = if rollout {
                        keyword_number(&tokens, "Score")
                    } else {
                        recognized
                            .then(|| tokens.get(9).and_then(|v| finite(v)))
                            .flatten()
                    };
                    record.candidates.push(ImportedCandidate {
                        rank: i as i32 + 1,
                        notation: tokens
                            .first()
                            .map(|v| sgf_notation(v, m.player))
                            .unwrap_or_default(),
                        equity,
                        equity_kind: if recognized
                            && tokens.get(10).is_some_and(|s| s.ends_with('C'))
                            || rollout && keyword_number(&tokens, "RC") == Some(1.0)
                        {
                            "cubeful"
                        } else if recognized
                            || rollout && keyword_number(&tokens, "RC") == Some(0.0)
                        {
                            "cubeless"
                        } else {
                            "unknown"
                        }
                        .into(),
                        probabilities: if recognized {
                            probabilities(
                                &(4..9)
                                    .map(|i| tokens.get(i).and_then(|s| finite(s)))
                                    .collect::<Vec<_>>(),
                            )
                        } else if rollout {
                            probabilities(&rollout_output(&tokens, None))
                        } else {
                            Value::Null
                        },
                        depth,
                        depth_plies: plies,
                        method: Some(
                            if recognized {
                                "evaluation"
                            } else if rollout || tokens.get(1) == Some(&"R") {
                                "rollout"
                            } else {
                                "unknown"
                            }
                            .into(),
                        ),
                        raw: json!({"value":value}),
                    });
                }
                if let Some(first) = record.candidates.first() {
                    record.depth = first.depth.clone();
                    record.depth_plies = first.depth_plies;
                    record.method = first.method.clone().unwrap_or("unknown".into());
                }
                record.raw = json!({"A":values,"AP":application});
                result.push(record);
            }
            if let Some(value) = text(node, "DA") {
                let mut record = base(m, "cube", engine, version.clone());
                let tokens: Vec<_> = value.split_whitespace().collect();
                let recognized = tokens.get(..3) == Some(&["E", "ver", "3"]);
                if recognized {
                    (record.depth, record.depth_plies) = gnubg_depth(tokens.get(3));
                    record.method = "evaluation".into();
                    let cubeful = tokens.get(3).is_some_and(|t| t.ends_with('C'));
                    record.cube = Some(
                        json!({"equity_kind":if !cubeful{"cubeless"}else if imported.match_length>0{"mwc"}else{"cubeful"},"perspective":"doubler","no_double":cubeful.then(||tokens.get(13).and_then(|v|finite(v))).flatten(),"double_take":cubeful.then(||tokens.get(20).and_then(|v|finite(v))).flatten()}),
                    );
                } else if tokens.get(..3) == Some(&["X", "ver", "3"]) {
                    record.method = "rollout".into();
                    record.depth = rollout_depth(&tokens);
                    let cubeful = keyword_number(&tokens, "RC");
                    record.cube = Some(
                        json!({"equity_kind":if cubeful==Some(0.0){"cubeless"}else if cubeful!=Some(1.0){"unknown"}else if imported.match_length>0{"mwc"}else{"cubeful"},"perspective":"doubler","no_double":(cubeful==Some(1.0)).then(||rollout_output(&tokens,Some("NoDouble"))[6]).flatten(),"double_take":(cubeful==Some(1.0)).then(||rollout_output(&tokens,Some("DoubleTake"))[6]).flatten()}),
                    );
                } else {
                    record.method = if tokens.first() == Some(&"R") {
                        "rollout"
                    } else {
                        "unknown"
                    }
                    .into();
                    record.cube = Some(json!({"equity_kind":"unknown"}));
                }
                record.raw = json!({"DA":value,"AP":application});
                result.push(record);
            }
        }
        if let Some(stats) = nodes[0].get("GS") {
            result.push(ImportedAnalysisData {
                game_no: game_index as i32 + 1,
                source_move_no: 0,
                checker_ply: None,
                position_id: None,
                gnubg_match_id: None,
                dice: None,
                kind: "statistics".into(),
                player: 0,
                played: "Game statistics".into(),
                engine: engine.into(),
                engine_version: version,
                method: "source_statistics".into(),
                depth: None,
                depth_plies: None,
                candidates: Vec::new(),
                cube: None,
                raw: json!({"GS":stats,"AP":application}),
            });
        }
    }
    Ok(result)
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportedAnalysisRecord {
    pub id: uuid::Uuid,
    pub match_uuid: uuid::Uuid,
    pub position_uuid: Option<uuid::Uuid>,
    pub source_format: String,
    #[serde(flatten)]
    pub data: ImportedAnalysisData,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportedAnalysisSummary {
    pub status: String,
    pub source_format: String,
    pub record_count: i64,
    pub checker_records: i64,
    pub cube_records: i64,
    pub engines: Vec<String>,
    pub depths: Vec<String>,
}
use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::HashMap;
use uuid::Uuid;
/// Persist in the same transaction as source bytes, games and checker positions.
pub(crate) async fn insert_records(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    records: &[ImportedAnalysisData],
) -> Result<()> {
    let rows: Vec<(Uuid, i32, i32)> =
        sqlx::query_as("SELECT id,game_no,ply FROM positions WHERE match_uuid=$1")
            .bind(id)
            .fetch_all(&mut **tx)
            .await?;
    let positions: HashMap<_, _> = rows.into_iter().map(|(id, g, p)| ((g, p), id)).collect();
    for (batch, records) in records.chunks(500).enumerate() {
        let mut q = sqlx::QueryBuilder::<Postgres>::new(
            "INSERT INTO imported_analyses(id,match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) ",
        );
        q.push_values(records.iter().enumerate(), |mut q, (i, r)| {
            q.push_bind(Uuid::new_v4())
                .push_bind(id)
                .push_bind(
                    r.checker_ply
                        .and_then(|p| positions.get(&(r.game_no, p)).copied()),
                )
                .push_bind((batch * 500 + i) as i32)
                .push_bind(r.game_no)
                .push_bind(r.source_move_no)
                .push_bind(&r.kind)
                .push_bind(serde_json::to_value(r).expect("source analysis is serializable"));
        });
        q.build().execute(&mut **tx).await?;
    }
    sqlx::query("UPDATE matches SET imported_analysis_loaded=true WHERE id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
/// Copy immutable source records with new row and position identities.
pub(crate) async fn copy_records(
    tx: &mut Transaction<'_, Postgres>,
    source: Uuid,
    target: Uuid,
    positions: &HashMap<Uuid, Uuid>,
) -> Result<()> {
    let rows:Vec<(Option<Uuid>,i32,i32,i32,String,Value)>=sqlx::query_as("SELECT position_uuid,source_ordinal,game_no,source_move_no,kind,data FROM imported_analyses WHERE match_uuid=$1 ORDER BY source_ordinal").bind(source).fetch_all(&mut **tx).await?;
    for (position, ordinal, game, mov, kind, data) in rows {
        sqlx::query("INSERT INTO imported_analyses(id,match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(Uuid::new_v4()).bind(target).bind(position.and_then(|p|positions.get(&p).copied())).bind(ordinal).bind(game).bind(mov).bind(kind).bind(data).execute(&mut **tx).await?;
    }
    sqlx::query("UPDATE matches SET imported_analysis_loaded=(SELECT imported_analysis_loaded FROM matches WHERE id=$1) WHERE id=$2").bind(source).bind(target).execute(&mut **tx).await?;
    Ok(())
}
/// Earlier libraries already retain exact originals. Catalog lazily from them,
/// while holding the match row lock so concurrent readers cannot duplicate rows.
pub(crate) async fn ensure_catalog(pool: &PgPool, id: Uuid) -> Result<bool> {
    let loaded: Option<bool> =
        sqlx::query_scalar("SELECT imported_analysis_loaded FROM matches WHERE id=$1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let Some(loaded) = loaded else {
        return Ok(false);
    };
    if loaded {
        return Ok(true);
    }
    let mut tx = pool.begin().await?;
    let row: Option<(bool, String, Vec<u8>, String)> = sqlx::query_as(
        "SELECT imported_analysis_loaded,format,original,sgf FROM matches WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((loaded, format, original, sgf)) = row else {
        return Ok(false);
    };
    if !loaded {
        let records = tokio::task::spawn_blocking(move || {
            let source = match format.as_str() {
                "xg" => crate::import::xg_to_sgf(&original)?,
                "bgf" => crate::import::bgf_to_sgf(&original)?,
                _ => sgf,
            };
            extract(&source)
        })
        .await??;
        insert_records(&mut tx, id, &records).await?;
    }
    tx.commit().await?;
    Ok(true)
}
pub async fn match_summary(pool: &PgPool, id: Uuid) -> Result<Option<ImportedAnalysisSummary>> {
    if !ensure_catalog(pool, id).await? {
        return Ok(None);
    }
    let format: String = sqlx::query_scalar("SELECT format FROM matches WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await?;
    let (count,checker,cube,engines,depths):(i64,i64,i64,Vec<String>,Vec<String>)=sqlx::query_as("SELECT count(*),count(*) FILTER(WHERE kind='checker'),count(*) FILTER(WHERE kind='cube'),coalesce(array_agg(DISTINCT data->>'engine') FILTER(WHERE data->>'engine' IS NOT NULL),'{}'::text[]),coalesce(array_agg(DISTINCT data->>'depth') FILTER(WHERE data->>'depth' IS NOT NULL),'{}'::text[]) FROM imported_analyses WHERE match_uuid=$1").bind(id).fetch_one(pool).await?;
    Ok(Some(ImportedAnalysisSummary {
        status: if checker + cube > 0 {
            "available"
        } else {
            "missing"
        }
        .into(),
        source_format: format,
        record_count: count,
        checker_records: checker,
        cube_records: cube,
        engines,
        depths,
    }))
}
async fn records_where(
    pool: &PgPool,
    match_id: Uuid,
    position: Option<Uuid>,
    mov: Option<(i32, i32)>,
) -> Result<Vec<ImportedAnalysisRecord>> {
    let rows:Vec<(Uuid,Uuid,Option<Uuid>,String,Value)>=sqlx::query_as("SELECT a.id,a.match_uuid,a.position_uuid,m.format,a.data FROM imported_analyses a JOIN matches m ON m.id=a.match_uuid WHERE a.match_uuid=$1 AND ($2::uuid IS NULL OR a.position_uuid=$2) AND ($3::integer IS NULL OR (a.game_no=$3 AND a.source_move_no=$4)) ORDER BY a.source_ordinal").bind(match_id).bind(position).bind(mov.map(|m|m.0)).bind(mov.map(|m|m.1)).fetch_all(pool).await?;
    rows.into_iter()
        .map(|(id, match_uuid, position_uuid, source_format, data)| {
            Ok(ImportedAnalysisRecord {
                id,
                match_uuid,
                position_uuid,
                source_format,
                data: serde_json::from_value(data)?,
            })
        })
        .collect()
}
pub async fn position_records(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<Vec<ImportedAnalysisRecord>>> {
    let match_id: Option<Uuid> = sqlx::query_scalar("SELECT match_uuid FROM positions WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    let Some(match_id) = match_id else {
        return Ok(None);
    };
    if !ensure_catalog(pool, match_id).await? {
        return Ok(None);
    }
    Ok(Some(records_where(pool, match_id, Some(id), None).await?))
}

pub(crate) async fn position_checker_records(
    pool: &PgPool,
    position: &crate::model::Position,
) -> Result<Vec<ImportedAnalysisData>> {
    if !ensure_catalog(pool, position.match_uuid).await? {
        return Ok(vec![]);
    }
    let rows: Vec<Value> = sqlx::query_scalar("SELECT data FROM imported_analyses WHERE match_uuid=$1 AND position_uuid=$2 AND kind='checker' ORDER BY source_ordinal")
        .bind(position.match_uuid).bind(position.id).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|data| serde_json::from_value(data).ok())
        .collect())
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/matches/{id}/imported-analysis", get(get_match))
        .route("/api/positions/{id}/imported-analysis", get(get_position))
        .route("/api/moves/{id}/imported-analysis", get(get_move))
}
#[utoipa::path(get,path="/api/matches/{id}/imported-analysis",params(("id"=Uuid,Path)),responses((status=200,body=ImportedAnalysisSummary),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Imported analysis")]
async fn get_match(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ImportedAnalysisSummary>> {
    crate::play::require_review_available(&s.pool, id).await?;
    Ok(Json(
        match_summary(&s.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}
#[utoipa::path(get,path="/api/positions/{id}/imported-analysis",params(("id"=Uuid,Path)),responses((status=200,body=Vec<ImportedAnalysisRecord>),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Imported analysis")]
async fn get_position(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ImportedAnalysisRecord>>> {
    crate::play::require_position_review_available(&s.pool, id).await?;
    Ok(Json(
        position_records(&s.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}
#[utoipa::path(get,path="/api/moves/{id}/imported-analysis",params(("id"=Uuid,Path)),responses((status=200,body=Vec<ImportedAnalysisRecord>),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Imported analysis")]
async fn get_move(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ImportedAnalysisRecord>>> {
    let row: Option<(Uuid, i32, i32)> =
        sqlx::query_as("SELECT match_uuid,game_no,move_no FROM match_moves WHERE id=$1")
            .bind(id)
            .fetch_optional(&s.pool)
            .await
            .map_err(anyhow::Error::from)?;
    let (match_id, game, mov) = row.ok_or_else(ApiError::missing)?;
    crate::play::require_review_available(&s.pool, match_id).await?;
    if !ensure_catalog(&s.pool, match_id).await? {
        return Err(ApiError::missing());
    }
    Ok(Json(
        records_where(&s.pool, match_id, None, Some((game, mov))).await?,
    ))
}
#[derive(OpenApi)]
#[openapi(
    paths(get_match, get_position, get_move),
    components(schemas(
        ImportedAnalysisSummary,
        ImportedAnalysisRecord,
        ImportedAnalysisData,
        ImportedCandidate
    ))
)]
pub struct ImportedAnalysisDoc;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bgf_numeric_strings_project_without_rewriting_native_precision() {
        let (_, moves) = parse_sgf_source_moves("(;FF[4]GM[6];W[31hefe])").unwrap();
        let records = bgf_analysis(
            &moves[0],
            json!({"ply":2,"moveAnalysis":[{"move":{"from":[8,6,-1,-1],"to":[5,5,-1,-1]},"eq":{"useMatchEquity":true,"equity":"0.00270277","matchEquity":"0.50011218","myWins":"0.49812975","myGammon":"0.10"}}]}),
        );
        assert_eq!(records[0].candidates[0].equity, Some(0.50011218));
        assert_eq!(records[0].candidates[0].probabilities["win"], 0.49812975);
        assert_eq!(records[0].candidates[0].probabilities["win_gammon"], 0.10);
        assert_eq!(
            records[0].candidates[0].raw["eq"]["matchEquity"],
            "0.50011218"
        );
    }
    #[test]
    fn missing_rollout_section_stays_unknown() {
        let tokens = vec![
            "X", "ver", "3", "NoDouble", "Output", "0.5", "0", "0", "0", "0", "0", "0.55",
        ];
        assert!(
            rollout_output(&tokens, Some("DoubleTake"))
                .iter()
                .all(Option::is_none)
        );
    }
    #[test]
    fn cubeless_da_does_not_mislabel_unused_cubeful_output_as_mwc() {
        let records=extract("(;FF[4]GM[6]MI[length:7];W[31hefe]DA[E ver 3 0 1 0 1 0.5 0.1 0.01 0.1 0.01 0.0 0.0 0.5 0.1 0.01 0.1 0.01 0.0 0.0])").unwrap();
        let cube = records[0].cube.as_ref().unwrap();
        assert_eq!(cube["equity_kind"], "cubeless");
        assert_eq!(cube["no_double"], Value::Null);
        assert_eq!(cube["double_take"], Value::Null);
    }
    #[test]
    fn bgf_candidate_depth_and_match_equity_keep_native_meaning() {
        let (_, moves) = parse_sgf_source_moves("(;FF[4]GM[6];W[31hefe])").unwrap();
        let records = bgf_analysis(
            &moves[0],
            json!({"ply":2,"moveAnalysis":[{"ply":4,"move":{"from":[8,6,-1,-1],"to":[5,5,-1,-1]},"eq":{"useMatchEquity":true,"equity":0.123456789012345,"matchEquity":0.543210987654321,"myWins":0.55}}]}),
        );
        assert_eq!(records[0].candidates[0].depth_plies, Some(4));
        assert_eq!(records[0].candidates[0].equity_kind, "mwc");
        assert_eq!(records[0].candidates[0].equity, Some(0.543210987654321));
        assert_eq!(records[0].engine, "Unknown (BGBlitz source)");
        assert_eq!(
            records[0].candidates[0].raw["eq"]["equity"],
            0.123456789012345
        );
    }
    #[test]
    fn native_sgf_preserves_candidates_cube_units_and_provenance() {
        let records = extract(include_str!("../tests/fixtures/analysis-game1.sgf")).unwrap();
        let first = records
            .iter()
            .find(|r| r.kind == "checker")
            .expect("source analysis must be extracted");
        assert_eq!(first.engine, "GNU Backgammon");
        assert_eq!(first.engine_version.as_deref(), Some("1.05.002"));
        assert_eq!(first.depth_plies, Some(0));
        assert_eq!(first.source_move_no, 1);
        assert_eq!(first.checker_ply, Some(1));
        assert_eq!(first.candidates[0].notation, "24/18 13/11");
        assert_eq!(first.candidates[0].equity, Some(0.013766));
        assert_eq!(first.candidates[0].equity_kind, "cubeful");
        let cube = records.iter().find(|r| r.kind == "cube").unwrap();
        assert_eq!(cube.cube.as_ref().unwrap()["equity_kind"], "mwc");
        assert!(first.raw["A"].is_array());
        assert!(
            records
                .iter()
                .any(|r| r.played == "Drop" && r.checker_ply.is_none())
        );
    }
    #[test]
    fn cube_analysis_keeps_pre_roll_context_separate_from_checker_dice() {
        use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
        let records = extract(include_str!("../tests/fixtures/analysis-game1.sgf")).unwrap();
        let record = records
            .iter()
            .find(|r| r.kind == "cube" && r.checker_ply.is_some())
            .unwrap();
        let value = serde_json::to_value(record).unwrap();
        assert_eq!(value["dice"], json!([0, 0]));
        let bytes = STANDARD_NO_PAD
            .decode(value["gnubg_match_id"].as_str().unwrap())
            .unwrap();
        for bit in 15..21 {
            assert_eq!(bytes[bit / 8] & (1 << (bit % 8)), 0);
        }
        assert!(value["position_id"].is_string());
    }
    #[test]
    fn native_gnubg_rollout_keeps_score_trials_and_mwc_cube_outputs() {
        let records=extract("(;FF[4]GM[6]AP[GNU Backgammon:1.05.002]MI[length:7];W[31hefe]A[0][hefe X ver 3 Score 0.123456789 0.02 Trials 1296 Output 0.55 0.1 0.01 0.08 0.005 0.12 0.56 StdDev 0 0 0 0 0 0 0 SK 0 RC 1]DA[X ver 3 Eq Trials 1296 NoDouble Output 0.55 0.1 0.01 0.08 0.005 0.12 0.56 StdDev 0 0 0 0 0 0 0 DoubleTake Output 0.55 0.1 0.01 0.08 0.005 0.12 0.57 StdDev 0 0 0 0 0 0 0 SK 0 RC 1])").unwrap();
        assert_eq!(records[0].method, "rollout");
        assert_eq!(records[0].depth.as_deref(), Some("Rollout (1296 trials)"));
        assert_eq!(records[0].candidates[0].equity, Some(0.123456789));
        assert_eq!(records[0].candidates[0].equity_kind, "cubeful");
        assert_eq!(records[1].method, "rollout");
        assert_eq!(records[1].cube.as_ref().unwrap()["no_double"], 0.56);
        assert_eq!(records[1].cube.as_ref().unwrap()["double_take"], 0.57);
        assert_eq!(records[1].cube.as_ref().unwrap()["equity_kind"], "mwc");
    }
    #[test]
    fn unanalysed_plain_sgf_has_no_fabricated_analysis() {
        assert!(extract("(;FF[4]GM[6];W[31hefe])").unwrap().is_empty());
    }
    #[test]
    fn xg_source_never_acquires_gnubg_depths() {
        let sgf = crate::import::xg_to_sgf(include_bytes!("../tests/fixtures/WC2014.xg")).unwrap();
        let records = extract(&sgf).unwrap();
        let first = records
            .iter()
            .find(|r| r.kind == "checker")
            .expect("XG analysis must be extracted");
        assert_eq!(first.engine, "eXtreme Gammon");
        assert_eq!(first.depth.as_deref(), Some("Opening Book V2"));
        assert_eq!(first.depth_plies, None);
        assert!(
            records
                .iter()
                .any(|r| r.depth.as_deref() == Some("XGRoller++"))
        );
        assert!(
            records
                .iter()
                .filter(|r| r.kind == "checker")
                .all(|r| r.engine != "GNU Backgammon")
        );
    }
}
