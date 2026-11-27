use super::*;

/// Whether a queued or running job belongs to one of the matches `$1`. The
/// status filter reads the few active jobs from `jobs_active_claim`, so the
/// cost does not depend on the number of matches or finished jobs.
const ANY_ACTIVE: &str = "SELECT EXISTS(SELECT 1 FROM jobs j LEFT JOIN positions p ON p.id=j.position_uuid WHERE coalesce(j.match_uuid,p.match_uuid)=ANY($1) AND j.status IN ('queued','running'))";

pub(super) async fn any_active(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    ids: &[Uuid],
) -> ApiResult<bool> {
    Ok(sqlx::query_scalar(ANY_ACTIVE)
        .bind(ids)
        .fetch_one(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?)
}

/// Support for the ignored large-library performance test: the plan of the
/// active-job check that guards deleting the given matches.
#[doc(hidden)]
pub async fn explain_any_active(pool: &PgPool, ids: &[Uuid]) -> anyhow::Result<serde_json::Value> {
    Ok(sqlx::query_scalar(&format!(
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {ANY_ACTIVE}"
    ))
    .bind(ids)
    .fetch_one(pool)
    .await?)
}

pub(super) fn action_item(
    id: Uuid,
    status: &str,
    created: Option<Uuid>,
    existing: Option<Uuid>,
) -> ActionItem {
    ActionItem {
        match_id: id,
        status: status.into(),
        created_id: created,
        existing_id: existing,
        error: None,
    }
}

pub(super) async fn action_one(
    pool: &PgPool,
    a: &LibraryAction,
    id: Uuid,
) -> ApiResult<ActionItem> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    // Lock database ownership before the source; deletion uses the same order.
    let source_db: Option<Uuid> =
        sqlx::query_scalar("SELECT database_id FROM matches WHERE id=$1 AND NOT collection_only")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    let Some(source_db) = source_db else {
        return Ok(action_item(id, "not_found", None, None));
    };
    let mut lock_dbs = vec![source_db];
    if let Some(target) = a.target_database_id {
        lock_dbs.push(target)
    }
    lock_dbs.sort();
    lock_dbs.dedup();
    sqlx::query("SELECT id FROM databases WHERE id=ANY($1) ORDER BY id FOR KEY SHARE")
        .bind(&lock_dbs)
        .fetch_all(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let source: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT database_id,sha256 FROM matches WHERE id=$1 AND NOT collection_only FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let Some((source_db, hash)) = source else {
        return Ok(action_item(id, "not_found", None, None));
    };
    if a.database_id.is_some_and(|scope| scope != source_db) {
        return Ok(action_item(id, "wrong_database", None, None));
    }
    if source_db == PAUSED_MATCHES_DATABASE_ID && a.action != "tags" {
        return Ok(ActionItem {
            error: Some("Resume the paused match before copying, moving or deleting it".into()),
            ..action_item(id, "invalid", None, None)
        });
    }
    let item = match a.action.as_str() {
        "copy" | "move" => {
            let target = a.required_destination()?;
            if source_db == target {
                return Ok(action_item(id, "same_database", None, Some(id)));
            }
            // The unique constraint is the final arbiter for concurrent imports/copies.
            let existing: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM matches WHERE database_id=$1 AND sha256=$2")
                    .bind(target)
                    .bind(&hash)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(anyhow::Error::from)?;
            if let Some(existing) = existing {
                return Ok(action_item(id, "duplicate", None, Some(existing)));
            }
            if a.action == "move" {
                let moved = sqlx::query("UPDATE matches SET database_id=$2 WHERE id=$1")
                    .bind(id)
                    .bind(target)
                    .execute(&mut *tx)
                    .await;
                match moved {
                    Ok(_) => action_item(id, "moved", None, None),
                    Err(e)
                        if e.as_database_error()
                            .is_some_and(|e| e.is_unique_violation()) =>
                    {
                        tx.rollback().await.map_err(anyhow::Error::from)?;
                        let existing = sqlx::query_scalar(
                            "SELECT id FROM matches WHERE database_id=$1 AND sha256=$2",
                        )
                        .bind(target)
                        .bind(hash)
                        .fetch_optional(pool)
                        .await
                        .map_err(anyhow::Error::from)?;
                        return Ok(action_item(id, "duplicate", None, existing));
                    }
                    Err(e) => return Err(anyhow::Error::from(e).into()),
                }
            } else {
                let new_id = Uuid::new_v4();
                let inserted=sqlx::query_scalar::<_,Uuid>("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,created_at,database_id,display_name,played_on,tournament,round,place,result_state,white_score,black_score,metadata_version,moves_cataloged,source_metadata_loaded,native_match_context_pending,source_revision,play_mode,play_assisted,play_human_side,player_source_platform,import_warnings) SELECT $2,filename,format,sha256,white,black,match_length,position_count,original,sgf,created_at,$3,display_name,played_on,tournament,round,place,result_state,white_score,black_score,metadata_version,moves_cataloged,source_metadata_loaded,native_match_context_pending,source_revision,play_mode,play_assisted,play_human_side,player_source_platform,import_warnings FROM matches WHERE id=$1 ON CONFLICT(database_id,sha256) DO NOTHING RETURNING id").bind(id).bind(new_id).bind(target).fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?;
                if inserted.is_none() {
                    let existing = sqlx::query_scalar(
                        "SELECT id FROM matches WHERE database_id=$1 AND sha256=$2",
                    )
                    .bind(target)
                    .bind(hash)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(anyhow::Error::from)?;
                    return Ok(action_item(id, "duplicate", None, existing));
                }
                copy_children(&mut tx, id, new_id).await?;
                action_item(id, "copied", Some(new_id), None)
            }
        }
        "delete" => {
            if any_active(&mut tx, &[id]).await? {
                return Ok(action_item(id, "busy", None, None));
            }
            crate::position_retention::preserve(&mut tx, id).await?;
            sqlx::query("DELETE FROM matches WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?;
            action_item(id, "deleted", None, None)
        }
        "tags" => {
            if let Err(message) =
                crate::annotations::update_match_tags(&mut tx, id, &a.add_tags, &a.remove_tags)
                    .await?
            {
                return Ok(ActionItem {
                    error: Some(message.into()),
                    ..action_item(id, "invalid", None, None)
                });
            }
            action_item(id, "tagged", None, None)
        }
        _ => unreachable!(),
    };
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(item)
}

pub(super) async fn copy_children(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    source: Uuid,
    target: Uuid,
) -> ApiResult<()> {
    // Each table is copied in one statement. Statement triggers on these
    // tables (played-record fingerprint, analysis revisions) then run once
    // per table instead of once per copied row.
    let positions = new_ids(tx, "SELECT id FROM positions WHERE match_uuid=$1", source).await?;
    let (old_positions, new_positions) = id_arrays(&positions);
    sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) SELECT m.new,$3,p.ordinal,p.game_no,p.ply,p.player,p.dice,p.board,p.position_id,p.gnubg_match_id,p.notation FROM unnest($1::uuid[],$2::uuid[]) AS m(old,new) JOIN positions p ON p.id=m.old ORDER BY p.ordinal")
        .bind(&old_positions).bind(&new_positions).bind(target)
        .execute(&mut **tx).await.map_err(anyhow::Error::from)?;
    let games = new_ids(tx, "SELECT id FROM match_games WHERE match_uuid=$1", source).await?;
    let (old_games, new_games) = id_arrays(&games);
    sqlx::query("INSERT INTO match_games(id,match_uuid,game_no,position_count,white_score,black_score,crawford,cube_enabled) SELECT m.new,$3,g.game_no,g.position_count,g.white_score,g.black_score,g.crawford,g.cube_enabled FROM unnest($1::uuid[],$2::uuid[]) AS m(old,new) JOIN match_games g ON g.id=m.old")
        .bind(&old_games).bind(&new_games).bind(target)
        .execute(&mut **tx).await.map_err(anyhow::Error::from)?;
    crate::imported_analysis::copy_records(tx, source, target, &positions).await?;
    let moves = new_ids(tx, "SELECT id FROM match_moves WHERE match_uuid=$1", source).await?;
    let (old_moves, new_moves) = id_arrays(&moves);
    sqlx::query("INSERT INTO match_moves(id,match_uuid,game_no,move_no,kind,player,notation,position_uuid,board,dice,position_id,gnubg_match_id) SELECT m.new,$3,mv.game_no,mv.move_no,mv.kind,mv.player,mv.notation,pm.new,mv.board,mv.dice,mv.position_id,mv.gnubg_match_id FROM unnest($1::uuid[],$2::uuid[]) AS m(old,new) JOIN match_moves mv ON mv.id=m.old LEFT JOIN unnest($4::uuid[],$5::uuid[]) AS pm(old,new) ON pm.old=mv.position_uuid ORDER BY mv.game_no,mv.move_no")
        .bind(&old_moves).bind(&new_moves).bind(target).bind(&old_positions).bind(&new_positions)
        .execute(&mut **tx).await.map_err(anyhow::Error::from)?;
    sqlx::query(&format!(
        "INSERT INTO annotations(id,match_uuid,position_uuid,game_uuid,move_uuid,tags,comment,version,updated_at) SELECT gen_random_uuid(),CASE WHEN a.match_uuid IS NULL THEN NULL ELSE $1::uuid END,pm.new,gm.new,mm.new,a.tags,a.comment,a.version,a.updated_at FROM {} LEFT JOIN unnest($3::uuid[],$4::uuid[]) AS pm(old,new) ON pm.old=a.position_uuid LEFT JOIN unnest($5::uuid[],$6::uuid[]) AS gm(old,new) ON gm.old=a.game_uuid LEFT JOIN unnest($7::uuid[],$8::uuid[]) AS mm(old,new) ON mm.old=a.move_uuid WHERE {}=$2",
        crate::annotations::ANNOTATION_MATCH_JOIN_SQL,
        crate::annotations::ANNOTATION_MATCH_UUID_EXPR,
    ))
        .bind(target).bind(source)
        .bind(&old_positions).bind(&new_positions).bind(&old_games).bind(&new_games).bind(&old_moves).bind(&new_moves)
        .execute(&mut **tx).await.map_err(anyhow::Error::from)?;
    // Completed snapshots only: queued/running/failed work and learning state stay with the source.
    let jobs:Vec<(Uuid,Option<Uuid>,Option<Uuid>)>=sqlx::query_as("SELECT j.id,j.match_uuid,j.position_uuid FROM jobs j LEFT JOIN positions p ON p.id=j.position_uuid WHERE coalesce(j.match_uuid,p.match_uuid)=$1 AND j.status='completed' ORDER BY j.created_at,j.id").bind(source).fetch_all(&mut **tx).await.map_err(anyhow::Error::from)?;
    let mut copied_jobs = HashMap::new();
    for (old, mat, pos) in jobs {
        let new_id = Uuid::new_v4();
        copied_jobs.insert(old, new_id);
        sqlx::query("INSERT INTO jobs SELECT (jsonb_populate_record(NULL::jobs,to_jsonb(j)||jsonb_build_object('id',$2::uuid,'match_uuid',$3::uuid,'position_uuid',$4::uuid,'lease_token',NULL,'lease_until',NULL))).* FROM jobs j WHERE id=$1").bind(old).bind(new_id).bind(mat.map(|_|target)).bind(pos.and_then(|id|positions.get(&id).copied())).execute(&mut **tx).await.map_err(anyhow::Error::from)?;
        sqlx::query("INSERT INTO job_events(job_id,event,data) SELECT id,'completed',jsonb_build_object('status','completed','progress',progress,'error',NULL,'completed_games',completed_games,'total_games',total_games) FROM jobs WHERE id=$1")
            .bind(new_id).execute(&mut **tx).await.map_err(anyhow::Error::from)?;
        sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) SELECT $2,game_no,summary FROM match_analysis_games WHERE job_id=$1").bind(old).bind(new_id).execute(&mut **tx).await.map_err(anyhow::Error::from)?;
        // Source order is kept: the new serial IDs follow the original IDs.
        sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) SELECT $2,pm.new,a.game_no,a.decision_no,a.player,a.kind,a.equity_loss,a.data FROM move_analyses a LEFT JOIN unnest($3::uuid[],$4::uuid[]) AS pm(old,new) ON pm.old=a.position_uuid WHERE a.job_id=$1 ORDER BY a.id")
            .bind(old).bind(new_id).bind(&old_positions).bind(&new_positions)
            .execute(&mut **tx).await.map_err(anyhow::Error::from)?;
    }
    let selected: Option<(String, i64)> = sqlx::query_as(
        "SELECT analysis_id,source_revision FROM analysis_selections WHERE match_id=$1",
    )
    .bind(source)
    .fetch_optional(&mut **tx)
    .await
    .map_err(anyhow::Error::from)?;
    if let Some((selected, source_revision)) = selected {
        let copied_selection = if selected == format!("imported:{source}") {
            format!("imported:{target}")
        } else {
            selected
                .strip_prefix("job:")
                .and_then(|id| Uuid::parse_str(id).ok())
                .and_then(|id| copied_jobs.get(&id))
                .map(|id| format!("job:{id}"))
                .unwrap_or(selected)
        };
        // Retain stale selection metadata too: copying cannot silently promote
        // an old imported context or switch to another completed result.
        sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id,source_revision) VALUES($1,$2,$3)")
            .bind(target).bind(copied_selection).bind(source_revision)
            .execute(&mut **tx).await.map_err(anyhow::Error::from)?;
    }
    Ok(())
}

/// A fresh ID for each row the query returns, keyed by the source row's ID.
pub(super) async fn new_ids(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    query: &'static str,
    source: Uuid,
) -> ApiResult<HashMap<Uuid, Uuid>> {
    let ids: Vec<Uuid> = sqlx::query_scalar(query)
        .bind(source)
        .fetch_all(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(ids.into_iter().map(|old| (old, Uuid::new_v4())).collect())
}

/// Parallel source and copy ID arrays for `unnest($old, $new)`.
pub(super) fn id_arrays(ids: &HashMap<Uuid, Uuid>) -> (Vec<Uuid>, Vec<Uuid>) {
    ids.iter().map(|(old, new)| (*old, *new)).unzip()
}
