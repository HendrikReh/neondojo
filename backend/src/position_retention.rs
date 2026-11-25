//! Bounded provenance hosts for saved occurrences whose source match is deleted.
use anyhow::Result;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(crate) const COLLECTION_DATABASE_ID: Uuid = Uuid::from_u128(2);

/// Lock a position's current owner, retrying when deletion moved the saved
/// occurrence while the original owner's lock was being acquired.
pub(crate) async fn lock_owner(
    tx: &mut Transaction<'_, Postgres>,
    position: Uuid,
) -> Result<Option<Uuid>> {
    loop {
        let owner = sqlx::query_scalar("SELECT m.id FROM matches m JOIN positions p ON p.match_uuid=m.id WHERE p.id=$1 FOR KEY SHARE OF m")
            .bind(position).fetch_optional(&mut **tx).await?;
        if owner.is_some() {
            return Ok(owner);
        }
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM positions WHERE id=$1)")
            .bind(position)
            .fetch_one(&mut **tx)
            .await?;
        if !exists {
            return Ok(None);
        }
    }
}

/// The caller holds the source match FOR UPDATE and has checked active jobs.
/// Move only saved positions and their relevant children before its cascade.
pub(crate) async fn preserve(tx: &mut Transaction<'_, Postgres>, source: Uuid) -> Result<()> {
    let positions: Vec<Uuid> = sqlx::query_scalar("SELECT p.id FROM positions p JOIN saved_positions s ON s.position_uuid=p.id WHERE p.match_uuid=$1 ORDER BY p.id FOR UPDATE OF p")
        .bind(source).fetch_all(&mut **tx).await?;
    if positions.is_empty() {
        return Ok(());
    }
    // Early imports may still have no game catalog. Recover their rules before
    // the deletion removes the only copy of the SGF.
    let uncataloged: Option<String> = sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1 AND sgf<>'' AND NOT EXISTS(SELECT FROM match_games WHERE match_uuid=$1)")
        .bind(source).fetch_optional(&mut **tx).await?;
    if let Some(sgf) = uncataloged {
        crate::store::catalog_source_games(tx, source, sgf).await?;
    }
    // Game annotation PUT locks only its game target, not the parent match.
    // Serialize it before cloning/retargeting so a successful first save cannot
    // land on the old game after the annotation move and disappear in its cascade.
    sqlx::query("SELECT id FROM match_games WHERE match_uuid=$1 AND game_no IN (SELECT game_no FROM positions WHERE id=ANY($2)) ORDER BY game_no FOR UPDATE")
        .bind(source).bind(&positions).fetch_all(&mut **tx).await?;
    let host = Uuid::new_v4();
    sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,created_at,database_id,display_name,played_on,tournament,round,place,result_state,white_score,black_score,metadata_version,moves_cataloged,source_metadata_loaded,imported_analysis_loaded,collection_only,source_match_id,source_match_no,source_database_id,source_database_name,play_mode,play_assisted,play_human_side,source_revision,import_warnings) SELECT $2,m.filename,m.format,$3,m.white,m.black,m.match_length,$4,''::bytea,'',m.created_at,$5,m.display_name,m.played_on,m.tournament,m.round,m.place,m.result_state,m.white_score,m.black_score,m.metadata_version,true,true,true,true,m.id,m.match_no,m.database_id,d.name,m.play_mode,m.play_assisted,m.play_human_side,m.source_revision,m.import_warnings FROM matches m JOIN databases d ON d.id=m.database_id WHERE m.id=$1")
        .bind(source).bind(host).bind(format!("saved:{host}")).bind(positions.len() as i32).bind(COLLECTION_DATABASE_ID).execute(&mut **tx).await?;

    // Clone just the games needed by the retained occurrences. Legacy rows with
    // no game catalog still have their exact score/cube context in GNU IDs.
    sqlx::query("INSERT INTO match_games(match_uuid,game_no,position_count,white_score,black_score,crawford,cube_enabled) SELECT $2,p.game_no,count(*)::integer,coalesce(max(g.white_score),max(p.source_white_score),0),coalesce(max(g.black_score),max(p.source_black_score),0),coalesce(bool_or(g.crawford),bool_or(p.source_crawford),false),bool_and(g.cube_enabled) FROM positions p LEFT JOIN match_games g ON g.match_uuid=p.match_uuid AND g.game_no=p.game_no WHERE p.id=ANY($1) GROUP BY p.game_no")
        .bind(&positions).bind(host).execute(&mut **tx).await?;
    sqlx::query("UPDATE annotations a SET game_uuid=new.id FROM match_games old JOIN match_games new ON new.match_uuid=$2 AND new.game_no=old.game_no WHERE old.match_uuid=$1 AND a.game_uuid=old.id")
        .bind(source).bind(host).execute(&mut **tx).await?;
    sqlx::query("UPDATE annotations SET match_uuid=$2 WHERE match_uuid=$1")
        .bind(source)
        .bind(host)
        .execute(&mut **tx)
        .await?;
    // Stable move IDs keep move annotations; unrelated source events cascade.
    sqlx::query(
        "UPDATE match_moves SET match_uuid=$2 WHERE match_uuid=$1 AND position_uuid=ANY($3)",
    )
    .bind(source)
    .bind(host)
    .bind(&positions)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE imported_analyses SET match_uuid=$2 WHERE match_uuid=$1 AND position_uuid=ANY($3)",
    )
    .bind(source)
    .bind(host)
    .bind(&positions)
    .execute(&mut **tx)
    .await?;

    let jobs: Vec<Uuid> = sqlx::query_scalar("SELECT j.id FROM jobs j WHERE j.match_uuid=$1 AND EXISTS(SELECT 1 FROM move_analyses ma WHERE ma.job_id=j.id AND ma.position_uuid=ANY($2)) ORDER BY j.id")
        .bind(source).bind(&positions).fetch_all(&mut **tx).await?;
    sqlx::query("UPDATE jobs SET match_uuid=$2,result=CASE WHEN jsonb_typeof(result)='object' THEN jsonb_set(result,'{performance_rating}','null'::jsonb) ELSE result END WHERE id=ANY($1)")
        .bind(&jobs).bind(host).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM move_analyses WHERE job_id=ANY($1) AND (position_uuid IS NULL OR NOT(position_uuid=ANY($2)))")
        .bind(&jobs).bind(&positions).execute(&mut **tx).await?;
    // Relevant checkpoints retain their original engine/depth/statistics as
    // historical provenance. They cannot be used as a new full-match result.
    sqlx::query("DELETE FROM match_analysis_games c WHERE c.job_id=ANY($1) AND NOT EXISTS(SELECT 1 FROM move_analyses ma WHERE ma.job_id=c.job_id AND ma.game_no=c.game_no)")
        .bind(&jobs).execute(&mut **tx).await?;
    sqlx::query("UPDATE positions SET match_uuid=$2 WHERE id=ANY($1)")
        .bind(&positions)
        .bind(host)
        .execute(&mut **tx)
        .await?;
    // Keep the exact persisted selection when its result survived retention.
    // A no-longer-applicable selection remains explicit rather than switching source.
    sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id,updated_at,source_revision) SELECT $2,CASE WHEN analysis_id='imported:'||$1::text THEN 'imported:'||$2::text ELSE analysis_id END,updated_at,source_revision FROM analysis_selections WHERE match_id=$1")
        .bind(source).bind(host).execute(&mut **tx).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
