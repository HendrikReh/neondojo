//! Anki's stable v11 collection schema in a standard APKG ZIP, built entirely in Rust.
use super::super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rusqlite::{Connection, params};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    io::{Cursor, Write as _},
};
use zip::write::SimpleFileOptions;

const MODEL_ID: i64 = 1731000001;
pub(super) const MODEL_NAME: &str = "NeonDojo Decision v1";
const SCHEMA: &str = r#"
CREATE TABLE col(id integer primary key,crt integer not null,mod integer not null,scm integer not null,ver integer not null,dty integer not null,usn integer not null,ls integer not null,conf text not null,models text not null,decks text not null,dconf text not null,tags text not null);
CREATE TABLE notes(id integer primary key,guid text not null,mid integer not null,mod integer not null,usn integer not null,tags text not null,flds text not null,sfld integer not null,csum integer not null,flags integer not null,data text not null);
CREATE TABLE cards(id integer primary key,nid integer not null,did integer not null,ord integer not null,mod integer not null,usn integer not null,type integer not null,queue integer not null,due integer not null,ivl integer not null,factor integer not null,reps integer not null,lapses integer not null,left integer not null,odue integer not null,odid integer not null,flags integer not null,data text not null);
CREATE TABLE revlog(id integer primary key,cid integer not null,usn integer not null,ease integer not null,ivl integer not null,lastIvl integer not null,factor integer not null,time integer not null,type integer not null);
CREATE TABLE graves(usn integer not null,oid integer not null,type integer not null);
CREATE INDEX ix_notes_usn ON notes(usn); CREATE INDEX ix_cards_usn ON cards(usn); CREATE INDEX ix_revlog_usn ON revlog(usn); CREATE INDEX ix_cards_nid ON cards(nid); CREATE INDEX ix_cards_sched ON cards(did,queue,due); CREATE INDEX ix_revlog_cid ON revlog(cid); CREATE INDEX ix_notes_csum ON notes(csum);
"#;

pub(super) struct ExportCard {
    pub id: Uuid,
    signature: String,
    guid: String,
    fields: String,
    tags: String,
    filename: String,
    image: String,
    structured: String,
    pub modified: i64,
}
impl ExportCard {
    pub fn fingerprint(&self) -> String {
        let mut digest = Sha256::new();
        for part in [&self.fields, &self.tags, &self.image, &self.structured] {
            digest.update((part.len() as u64).to_be_bytes());
            digest.update(part.as_bytes());
        }
        format!("{:x}", digest.finalize())
    }
}
pub(super) fn prepare(
    cards: Vec<(Uuid, CardContent, CardAnswer)>,
    locale: ConfigurationLocale,
) -> anyhow::Result<Vec<ExportCard>> {
    let labels = Labels::new(locale);
    cards
        .into_iter()
        .map(|(id, content, answer)| {
            let signature = card_signature(&content);
            let digest = Sha256::digest(signature.as_bytes());
            let guid = URL_SAFE_NO_PAD.encode(&digest[..12]);
            let filename = format!("neondojo-{guid}.svg");
            let fields = [
                identity_field(&signature),
                question(&content, &filename, &labels)?,
                answer_html(&content, &answer, &labels),
            ]
            .join("\x1f");
            let tags = answer
                .tags
                .iter()
                .map(|t| t.split_whitespace().collect::<Vec<_>>().join("_"))
                .collect::<Vec<_>>()
                .join(" ");
            let image = board_svg(&content)?;
            let mut portable = content;
            CardSource::strip_local_refs(&mut portable);
            let structured = serde_json::to_string(&portable)?;
            Ok(ExportCard {
                id,
                signature,
                guid,
                fields,
                tags,
                filename,
                image,
                structured,
                modified: Utc::now().timestamp(),
            })
        })
        .collect()
}

pub(super) fn build(deck_id: Uuid, name: &str, cards: &[ExportCard]) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(cards.len() <= 1000, "Export exceeds 1000 cards");
    let timestamp = Utc::now().timestamp();
    let did = numeric_id(deck_id.as_bytes(), b"deck");
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("collection.anki2");
    let mut conn = Connection::open(&path)?;
    conn.execute_batch(SCHEMA)?;
    let tx = conn.transaction()?;
    let model = json!({"id":MODEL_ID,"name":MODEL_NAME,"type":0,"mod":timestamp,"usn":-1,"sortf":0,"did":did,"tags":[],"vers":[],"latexPre":"","latexPost":"","css":".card{font-family:Arial,sans-serif;font-size:18px;text-align:center;color:#182a24;background:#f8faf7}.board{width:100%;max-width:680px}table{margin:1em auto;border-collapse:collapse}th,td{padding:.35em .75em;border-bottom:1px solid #ccd3cd}.notes{white-space:pre-wrap;text-align:left;max-width:680px;margin:1em auto}.context{font-size:.85em;color:#52635b}.best{font-size:1.2em;font-weight:bold}","flds":[{"name":"Identity","ord":0,"sticky":false,"rtl":false,"font":"Arial","size":12,"media":[]},{"name":"Question","ord":1,"sticky":false,"rtl":false,"font":"Arial","size":20,"media":[]},{"name":"Answer","ord":2,"sticky":false,"rtl":false,"font":"Arial","size":20,"media":[]}],"tmpls":[{"name":"Decision","ord":0,"qfmt":"{{Question}}","afmt":"{{FrontSide}}<hr id=answer>{{Answer}}","bqfmt":"","bafmt":"","did":null}],"req":[[0,"all",[1]]]});
    let mut models = serde_json::Map::new();
    models.insert(MODEL_ID.to_string(), model);
    let deck = json!({"id":did,"name":name,"desc":"NeonDojo decisions.","mod":timestamp,"usn":-1,"dyn":0,"conf":1,"collapsed":false,"extendNew":0,"extendRev":50,"lrnToday":[0,0],"newToday":[0,0],"revToday":[0,0],"timeToday":[0,0]});
    let mut decks = serde_json::Map::new();
    decks.insert(did.to_string(), deck);
    let dconf = json!({"1":{"id":1,"name":"Default","mod":0,"usn":0,"maxTaken":60,"autoplay":true,"timer":0,"replayq":true,"new":{"delays":[1,10],"ints":[1,4,7],"initialFactor":2500,"order":1,"perDay":20,"bury":true,"separate":true},"rev":{"perDay":200,"ease4":1.3,"fuzz":0.05,"ivlFct":1.0,"maxIvl":36500,"minSpace":1,"bury":true},"lapse":{"delays":[10],"mult":0,"minInt":1,"leechFails":8,"leechAction":0}}});
    let conf = json!({"activeDecks":[did],"curDeck":did,"curModel":MODEL_ID,"nextPos":cards.len()+1,"newSpread":0,"collapseTime":1200,"timeLim":0,"estTimes":true,"dueCounts":true,"sortType":"noteFld","sortBackwards":false,"addToCur":true,"newBury":true});
    tx.execute(
        "INSERT INTO col VALUES(1,?1,?2,?2,11,0,-1,0,?3,?4,?5,?6,'{}')",
        params![
            timestamp,
            timestamp * 1000,
            conf.to_string(),
            serde_json::Value::Object(models).to_string(),
            serde_json::Value::Object(decks).to_string(),
            dconf.to_string()
        ],
    )?;
    let mut media = BTreeMap::new();
    let mut images = Vec::new();
    for (index, card) in cards.iter().enumerate() {
        let nid = numeric_id(card.signature.as_bytes(), b"note");
        let cid = numeric_id(card.signature.as_bytes(), b"card");
        tx.execute(
            "INSERT INTO notes VALUES(?1,?2,?3,?4,-1,?5,?6,?7,0,0,?8)",
            params![
                nid,
                card.guid,
                MODEL_ID,
                card.modified,
                format!(" {} ", card.tags),
                card.fields,
                card.signature,
                card.structured,
            ],
        )?;
        // Exports intentionally contain new Anki cards; local FSRS history remains in PostgreSQL.
        tx.execute(
            "INSERT INTO cards VALUES(?1,?2,?3,0,?4,-1,0,0,?5,0,0,0,0,0,0,0,0,'')",
            params![cid, nid, did, timestamp, index + 1],
        )?;
        media.insert(index.to_string(), &card.filename);
        images.push(&card.image);
    }
    tx.commit()?;
    drop(conn);
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("collection.anki2", options)?;
    zip.write_all(&std::fs::read(path)?)?;
    zip.start_file("media", options)?;
    zip.write_all(&serde_json::to_vec(&media)?)?;
    for (i, svg) in images.iter().enumerate() {
        zip.start_file(i.to_string(), options)?;
        zip.write_all(svg.as_bytes())?;
    }
    Ok(zip.finish()?.into_inner())
}
fn numeric_id(input: &[u8], namespace: &[u8]) -> i64 {
    let mut hash = Sha256::new();
    hash.update(namespace);
    hash.update(input);
    let digest = hash.finalize();
    (u64::from_be_bytes(digest[..8].try_into().unwrap()) & ((1 << 53) - 1)) as i64 + 1
}
pub(super) fn identity_field(signature: &str) -> String {
    html(signature)
}

fn html(input: &str) -> String {
    let mut output = String::new();
    for c in input.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(c),
        }
    }
    output
}
struct Labels {
    checker: &'static str,
    offer: &'static str,
    response: &'static str,
    best: &'static str,
    played: &'static str,
    notes: &'static str,
    engine: &'static str,
    score: &'static str,
    cube: &'static str,
    ply: &'static str,
    german_cube_actions: bool,
}
impl Labels {
    fn new(locale: ConfigurationLocale) -> Self {
        let words = match locale {
            ConfigurationLocale::En => [
                "What is the best checker play?",
                "Would you double or play on?",
                "The cube has been offered. Take or pass?",
                "Best play",
                "Played",
                "Notes",
                "Engine",
                "Score",
                "Cube",
                "ply",
            ],
            ConfigurationLocale::De => [
                "Was ist der beste Zug?",
                "Doppeln oder weiterspielen?",
                "Der Doppler wurde angeboten. Annehmen oder passen?",
                "Bester Zug",
                "Gespielt",
                "Notizen",
                "Engine",
                "Spielstand",
                "Doppler",
                "Ply",
            ],
        };
        Self {
            checker: words[0],
            offer: words[1],
            response: words[2],
            best: words[3],
            played: words[4],
            notes: words[5],
            engine: words[6],
            score: words[7],
            cube: words[8],
            ply: words[9],
            german_cube_actions: locale == ConfigurationLocale::De,
        }
    }

    fn action<'a>(&self, kind: &str, notation: &'a str) -> &'a str {
        match (self.german_cube_actions, kind, notation) {
            (true, "cube_offer", "double") => "Doppeln",
            (true, "cube_offer", "no double") => "Nicht doppeln",
            (true, "cube_response", "take") => "Annehmen",
            (true, "cube_response", "drop") => "Passen",
            _ => notation,
        }
    }
}
fn match_fields(id: &str) -> anyhow::Result<[i32; 7]> {
    use crate::backgammon::ids::{Field, MatchId};
    let id = MatchId::decode(id).ok_or_else(|| anyhow::anyhow!("Invalid match ID"))?;
    Ok([
        1 << id.get(Field::CUBE_LOG2),
        id.get(Field::CUBE_OWNER),
        id.get(Field::LENGTH),
        id.get(Field::SCORE_0),
        id.get(Field::SCORE_1),
        id.get(Field::CRAWFORD),
        1 - id.get(Field::NO_JACOBY),
    ])
}
fn question(c: &CardContent, filename: &str, l: &Labels) -> anyhow::Result<String> {
    let [cube, owner, length, white, black, crawford, jacoby] =
        match_fields(&c.position.gnubg_match_id)?;
    let prompt = match c.kind.as_str() {
        "cube_offer" => l.offer,
        "cube_response" => l.response,
        _ => l.checker,
    };
    let cube = if c.kind == "cube_response" {
        cube * 2
    } else {
        cube
    };
    let ownership = match owner {
        0 => "White",
        1 => "Black",
        _ => "–",
    };
    Ok(format!(
        "<h3>{prompt}</h3><p>{} (White) · {} (Black)</p><p class=\"context\">{}: {white}–{black} / {length} · {}: {cube} ({ownership}){}{} · {} {}</p><img class=\"board\" src=\"{filename}\" alt=\"Backgammon board\"><p class=\"context\">{}:{}</p>",
        html(&c.players[0]),
        html(&c.players[1]),
        l.score,
        l.cube,
        if crawford == 1 { " · Crawford" } else { "" },
        if length == 0 {
            if jacoby == 1 {
                " · Jacoby ✓"
            } else {
                " · Jacoby ✗"
            }
        } else {
            ""
        },
        c.position.game_no,
        if c.position.player == 0 {
            "White"
        } else {
            "Black"
        },
        html(&c.position.position_id),
        html(&c.position.gnubg_match_id)
    ))
}
fn answer_html(c: &CardContent, a: &CardAnswer, l: &Labels) -> String {
    let solution = &c.solution;
    let best = solution
        .decision
        .as_ref()
        .map(|d| d.best.as_str())
        .or_else(|| solution.candidates.first().map(|c| c.notation.as_str()))
        .unwrap_or("–");
    let mut out = format!(
        "<p class=\"best\">{}: {}</p><p>{}: {}</p>",
        l.best,
        html(l.action(&c.kind, best)),
        l.played,
        html(l.action(&c.kind, &a.played))
    );
    if let Some(d) = &solution.decision {
        let _ = write!(out, "<p>Δ equity: {:.6}</p>", d.equity_loss);
    }
    if !solution.candidates.is_empty() {
        out.push_str("<table><tbody>");
        for candidate in solution.candidates.iter().take(20) {
            let _ = write!(
                out,
                "<tr><td>{}</td><td>{}</td><td>{:+.6}</td></tr>",
                candidate.rank,
                html(&candidate.notation),
                candidate.equity
            );
        }
        out.push_str("</tbody></table>");
    }
    if !a.notes.is_empty() {
        let _ = write!(
            out,
            "<h4>{}</h4><div class=\"notes\">{}</div>",
            l.notes,
            html(&a.notes)
        );
    }
    let _ = write!(
        out,
        "<p class=\"context\">{}: {} · {} {} · {} · {}</p>",
        l.engine,
        html(&solution.engine),
        html(
            &solution
                .imported
                .as_ref()
                .and_then(|record| record.depth.clone())
                .or_else(|| solution.plies.map(|plies| plies.to_string()))
                .unwrap_or_default()
        ),
        if solution.imported.is_some() {
            ""
        } else {
            l.ply
        },
        html(&c.source.kind),
        c.source
            .imported_analysis_id
            .or(c.source.job_id)
            .map(|id| id.to_string())
            .unwrap_or_default()
    );
    out
}
fn board_svg(c: &CardContent) -> anyhow::Result<String> {
    let board = &c.position.board;
    anyhow::ensure!(board.len() == 26, "Invalid card board");
    let mut out = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 680 440"><rect width="680" height="440" rx="10" fill="#59402f"/><rect x="20" y="28" width="640" height="384" fill="#d7b689"/><rect x="325" y="28" width="30" height="384" fill="#59402f"/>"##,
    );
    let xs = |column: usize| 24.0 + column as f32 * 50.0 + if column >= 6 { 30.0 } else { 0.0 };
    for top in [true, false] {
        for column in 0..12 {
            let point = if top { 13 + column } else { 12 - column };
            let label = if c.position.player == 1 {
                25 - point
            } else {
                point
            };
            let x = xs(column);
            let (base, tip) = if top { (40, 194) } else { (400, 246) };
            let color = if point % 2 == 0 { "#94513d" } else { "#e9d9b9" };
            let _ = write!(
                out,
                "<path d=\"M {x} {base} L {} {base} L {} {tip} Z\" fill=\"{color}\"/><text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"13\" fill=\"#fff\">{label}</text>",
                x + 48.0,
                x + 24.0,
                x + 24.0,
                if top { 21 } else { 430 }
            );
            let n = board[point];
            for checker in 0..n.unsigned_abs().min(5) {
                let cy = if top {
                    60 + checker * 28
                } else {
                    380 - checker * 28
                };
                checker_svg(&mut out, x + 24.0, cy as f32, n > 0);
            }
            if n.unsigned_abs() > 5 {
                let _ = write!(
                    out,
                    "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-weight=\"bold\" font-size=\"16\" fill=\"{}\">{}</text>",
                    x + 24.0,
                    if top { 176 } else { 272 },
                    if n > 0 { "#151515" } else { "#fff" },
                    n.unsigned_abs()
                );
            }
        }
    }
    for (index, y) in [(0, 160), (25, 280)] {
        let n = board[index];
        if n != 0 {
            checker_svg(&mut out, 340.0, y as f32, n > 0);
            let _ = write!(
                out,
                "<text x=\"340\" y=\"{}\" text-anchor=\"middle\" fill=\"{}\" font-size=\"16\">{}</text>",
                y + 5,
                if n > 0 { "#111" } else { "#fff" },
                n.unsigned_abs()
            );
        }
    }
    let dice = if c.kind == "checker" {
        format!("{} · {}", c.position.dice[0], c.position.dice[1])
    } else {
        "◆".into()
    };
    let _ = write!(
        out,
        "<rect x=\"277\" y=\"204\" width=\"126\" height=\"32\" rx=\"7\" fill=\"#f8f3e6\"/><text x=\"340\" y=\"227\" text-anchor=\"middle\" font-size=\"24\" fill=\"#222\">{dice}</text></svg>"
    );
    Ok(out)
}
fn checker_svg(out: &mut String, x: f32, y: f32, white: bool) {
    let _ = write!(
        out,
        "<circle cx=\"{x}\" cy=\"{y}\" r=\"13\" fill=\"{}\" stroke=\"#28231e\" stroke-width=\"1.5\"/>",
        if white { "#faf5e8" } else { "#252b29" }
    );
}

#[cfg(test)]
mod tests;
