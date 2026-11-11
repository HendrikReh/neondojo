//! A live session owns one ordinary library match. Deletion permanently detaches it.
use super::{PlayPhase, PlaySession, named_export};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub const BOT_DATABASE_ID: Uuid = Uuid::from_u128(3);

pub async fn lock_bot_database(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM databases WHERE id=$1 FOR UPDATE")
        .bind(BOT_DATABASE_ID)
        .fetch_one(&mut **tx)
        .await?;
    Ok(())
}

/// Lock ownership in the same order as database/match management, before the
/// session CAS (whose FK may otherwise deadlock with deleting its match).
pub async fn lock_match(
    tx: &mut Transaction<'_, Postgres>,
    session_id: Uuid,
) -> crate::api::ApiResult<Option<Uuid>> {
    let owner: Option<(Uuid, Uuid)> = sqlx::query_as("SELECT m.id,m.database_id FROM matches m JOIN play_sessions s ON s.library_match_id=m.id WHERE s.id=$1")
        .bind(session_id).fetch_optional(&mut **tx).await.map_err(anyhow::Error::from)?;
    let Some((id, db)) = owner else {
        return Ok(None);
    };
    sqlx::query("SELECT id FROM databases WHERE id=$1 FOR UPDATE")
        .bind(db)
        .execute(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?;
    let actual: Option<Uuid> =
        sqlx::query_scalar("SELECT database_id FROM matches WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(anyhow::Error::from)?;
    if actual.is_some_and(|actual| actual != db) {
        return Err(crate::api::ApiError(
            axum::http::StatusCode::CONFLICT,
            "Library match moved; retry the play action".into(),
        ));
    }
    Ok(actual.map(|_| id))
}

fn source(session: &PlaySession, sgf: &str) -> String {
    let shell = format!(
        "(;GM[6]FF[4]CA[UTF-8]PW[Human]PB[GNUbg]MI[length:{}][game:0][ws:0][bs:0])",
        session.match_length
    );
    let mut source = named_export(
        if sgf.is_empty() { &shell } else { sgf },
        &session.human_name,
        session.human_player,
    );
    // Distinguish otherwise identical empty/short matches for ordinary copy/import deduplication.
    if let Some(root) = source.find(';') {
        source.insert_str(root + 1, &format!("BTSESSION[{}]", session.id));
    }
    source
}

// GNUbg 1.05.002 loses SETBOARD's fPlayer on export and writes PL[W]
// even for Black. Archive a strict, interoperable SGF rather than requiring
// every ordinary library consumer to know this native exporter quirk.
pub(super) fn normalize_native_setup_turn(source: String) -> Result<String> {
    let mut trees = crate::import::sgf::parse_trees(&source)?;
    let mut changed = false;
    for nodes in &mut trees {
        for index in 0..nodes.len() {
            if !["AE", "AW", "AB"]
                .iter()
                .any(|key| nodes[index].contains_key(*key))
            {
                continue;
            }
            let player = nodes[index..].iter().find_map(|node| {
                if node.contains_key("W") {
                    Some("W")
                } else if node.contains_key("B") {
                    Some("B")
                } else {
                    None
                }
            });
            if let Some(player) = player
                && nodes[index]
                    .get("PL")
                    .and_then(|value| value.first())
                    .map(String::as_str)
                    != Some(player)
            {
                nodes[index].insert("PL".into(), vec![player.into()]);
                changed = true;
            }
        }
    }
    if !changed {
        return Ok(source);
    }
    let (_, native_context) = crate::import::sgf::parse_native_sgf_with_context(&source)?;
    let output = crate::import::sgf::serialize_trees(&trees);
    let (_, strict_context) = crate::import::sgf::parse_sgf_with_context(&output)?;
    ensure!(
        native_context == strict_context,
        "Native setup normalization changed a decision context"
    );
    Ok(output)
}

pub async fn sync(
    tx: &mut Transaction<'_, Postgres>,
    session: &PlaySession,
    sgf: &str,
    existing: Option<Uuid>,
) -> Result<Uuid> {
    let sgf = normalize_native_setup_turn(source(session, sgf))?;
    let (imported, _) = crate::import::sgf::parse_sgf_with_context(&sgf)?;
    let id = existing.unwrap_or_else(Uuid::new_v4);
    let hash = format!("{:x}", Sha256::digest(sgf.as_bytes()));
    let result = if matches!(session.phase, PlayPhase::MatchOver)
        || super::reports::money_finished(session)
    {
        "completed"
    } else {
        "ongoing"
    };
    let filename = format!("Bot match {}.sgf", session.id);
    if existing.is_none() {
        sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,database_id,created_at,played_on,result_state,white_score,black_score,source_metadata_loaded,imported_analysis_loaded) SELECT $1,$2,'sgf',$3,$4,$5,$6,0,$7,$8,$9,created_at,created_at::date,$10,$11,$12,true,true FROM play_sessions WHERE id=$13")
            .bind(id).bind(filename).bind(&hash).bind(&imported.players[0]).bind(&imported.players[1]).bind(session.match_length).bind(sgf.as_bytes()).bind(&sgf).bind(BOT_DATABASE_ID).bind(result).bind(session.scores[0]).bind(session.scores[1]).bind(session.id).execute(&mut **tx).await?;
    }
    update_mode_metadata(tx, session, id).await?;
    let old_source: String = sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1")
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    if old_source != sgf {
        // A worker must not append checkpoints for a superseded source. Preserve
        // completed history and position analyses, fence in-flight match jobs.
        sqlx::query("WITH invalidated AS (UPDATE jobs SET status='failed',error='Match advanced during analysis; analyze the current match again',lease_token=NULL,lease_until=NULL,updated_at=now() WHERE match_uuid=$1 AND kind='match' AND status IN ('queued','running') RETURNING id,progress,error,total_games,completed_games) INSERT INTO job_events(job_id,event,data) SELECT id,'failed',jsonb_build_object('status','failed','progress',progress,'error',error,'total_games',total_games,'completed_games',completed_games) FROM invalidated")
            .bind(id).execute(&mut **tx).await?;
    }
    let stored: Vec<crate::model::Position> =
        sqlx::query_as("SELECT * FROM positions WHERE match_uuid=$1 ORDER BY ordinal")
            .bind(id)
            .fetch_all(&mut **tx)
            .await?;
    ensure!(
        stored.len() <= imported.positions.len(),
        "Native play source removed existing decisions"
    );
    for (ordinal, (old, new)) in stored.iter().zip(&imported.positions).enumerate() {
        ensure!(
            old.ordinal == ordinal as i32
                && old.game_no == new.game_no
                && old.ply == new.ply
                && old.player == new.player
                && old.dice == new.dice
                && old.board == new.board
                && old.position_id == new.position_id
                && old.gnubg_match_id == new.match_id
                && old.notation == new.notation,
            "Native play source rewrote an existing decision"
        );
    }
    for (ordinal, p) in imported.positions.iter().enumerate().skip(stored.len()) {
        sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(match_uuid,ordinal) DO NOTHING")
            .bind(Uuid::new_v4()).bind(id).bind(ordinal as i32).bind(p.game_no).bind(p.ply).bind(p.player).bind(p.dice.to_vec()).bind(&p.board).bind(&p.position_id).bind(&p.match_id).bind(&p.notation).execute(&mut **tx).await?;
    }
    for game in &imported.games {
        sqlx::query("INSERT INTO match_games(match_uuid,game_no,position_count,white_score,black_score,crawford) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(match_uuid,game_no) DO UPDATE SET position_count=EXCLUDED.position_count,white_score=EXCLUDED.white_score,black_score=EXCLUDED.black_score,crawford=EXCLUDED.crawford")
            .bind(id).bind(game.game_no).bind(game.position_count).bind(game.white_score).bind(game.black_score).bind(game.crawford).execute(&mut **tx).await?;
    }
    sqlx::query("UPDATE matches SET sha256=$2,original=$3,sgf=$4,white=$5,black=$6,position_count=$7,metadata_version=metadata_version+CASE WHEN result_state IS DISTINCT FROM $8 OR white_score IS DISTINCT FROM $9 OR black_score IS DISTINCT FROM $10 THEN 1 ELSE 0 END,result_state=$8,white_score=$9,black_score=$10,moves_cataloged=CASE WHEN sgf<>$4 THEN false ELSE moves_cataloged END,source_revision=source_revision+CASE WHEN sgf<>$4 THEN 1 ELSE 0 END WHERE id=$1")
        .bind(id).bind(hash).bind(sgf.as_bytes()).bind(&sgf).bind(&imported.players[0]).bind(&imported.players[1]).bind(imported.positions.len() as i32).bind(result).bind(session.scores[0]).bind(session.scores[1]).execute(&mut **tx).await?;
    if existing.is_none() {
        sqlx::query(
            "UPDATE play_sessions SET library_match_id=$2,library_archived=true WHERE id=$1",
        )
        .bind(session.id)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(id)
}

/// One transaction per legacy session; no native replay and no checkpoint or
/// revision changes. The archive marker also survives ON DELETE SET NULL.
pub async fn backfill(pool: &PgPool) -> Result<usize> {
    let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM play_sessions WHERE NOT library_archived AND library_match_id IS NULL ORDER BY id LIMIT 100").fetch_all(pool).await?;
    let mut count = 0;
    for id in ids {
        let mut tx = pool.begin().await?;
        lock_bot_database(&mut tx).await?;
        let row: Option<(sqlx::types::Json<PlaySession>, String)> = sqlx::query_as("SELECT state,sgf FROM play_sessions WHERE id=$1 AND NOT library_archived AND library_match_id IS NULL FOR UPDATE")
            .bind(id).fetch_optional(&mut *tx).await?;
        if let Some((session, sgf)) = row {
            sync(&mut tx, &session, &sgf, None).await?;
            count += 1;
        }
        tx.commit().await?;
    }
    Ok(count)
}

/// This context belongs to the match, including library copies and detachments.
pub(super) async fn update_mode_metadata(
    tx: &mut Transaction<'_, Postgres>,
    session: &PlaySession,
    id: Uuid,
) -> Result<()> {
    let assisted = session.learning_used
        || session.play_mode != super::PlayMode::Competition
        || session
            .pending_decision
            .iter()
            .chain(session.decisions.iter())
            .any(|d| d.assisted || d.revealed || d.warning);
    sqlx::query("UPDATE matches SET play_mode=$2,play_assisted=play_assisted OR $3 WHERE id=$1")
        .bind(id)
        .bind(session.play_mode)
        .bind(assisted)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
