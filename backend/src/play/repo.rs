use super::*;

#[derive(FromRow)]
pub(super) struct Saved {
    pub(super) state: sqlx::types::Json<PlaySession>,
    pub(super) gnubg_path: String,
    pub(super) sgf: String,
    pub(super) native_id: String,
    pub(super) paused_from_database_id: Option<Uuid>,
}

pub(super) const SAVED_STATE: &str = "state || jsonb_build_object('library_match_id', library_match_id, 'paused', paused_from_database_id IS NOT NULL)";

pub(super) async fn saved(state: &AppState, id: Uuid) -> ApiResult<Saved> {
    sqlx::query_as(&format!("SELECT {SAVED_STATE} AS state,gnubg_path,sgf,native_id,paused_from_database_id FROM play_sessions WHERE id=$1"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)
}

pub(super) async fn persist_session(
    state: &AppState,
    session: &PlaySession,
    sgf: &str,
    native_id: &str,
    version: i64,
    sync: bool,
) -> ApiResult<Json<PlaySession>> {
    persist_session_with_report(state, session, sgf, native_id, version, sync, None).await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn persist_session_with_report(
    state: &AppState,
    session: &PlaySession,
    sgf: &str,
    native_id: &str,
    version: i64,
    sync: bool,
    completed_report: Option<&PlaySession>,
) -> ApiResult<Json<PlaySession>> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let linked = library::lock_match(&mut tx, session.id, &[])
        .await?
        .map(|(id, _)| id);
    let mut current = session.clone();
    current.library_match_id = linked;
    if let Some(report) = completed_report {
        let mut captured = report.clone();
        captured.library_match_id = linked;
        reports::archive(&mut tx, &captured, sgf).await?;
    }
    if current.review.is_some()
        && (matches!(current.phase, PlayPhase::MatchOver) || reports::money_finished(&current))
    {
        let (report_id, report_version) = reports::archive(&mut tx, &current, sgf).await?;
        current.study_report_id = Some(report_id);
        current.study_report_version = Some(report_version);
        current.review_game_no = (current.match_length == 0).then_some(current.game_no);
    }
    let updated=sqlx::query("UPDATE play_sessions SET state=$1,sgf=$2,native_id=$3,version=version+1,updated_at=now() WHERE id=$4 AND version=$5")
        .bind(sqlx::types::Json(&current)).bind(sgf).bind(native_id).bind(session.id).bind(version)
        .execute(&mut *tx).await.map_err(anyhow::Error::from)?.rows_affected();
    if updated != 1 {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; reload before playing".into(),
        ));
    }
    if let Some(match_id) = linked {
        if sync {
            library::sync(
                &mut tx,
                &current,
                sgf,
                library::ArchiveTarget::Existing(match_id),
            )
            .await?;
        } else {
            library::update_mode_metadata(&mut tx, &current, match_id).await?;
        }
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(present_with_results(current, sgf.to_owned()).await?))
}

pub(super) async fn recent_sessions(pool: &sqlx::PgPool) -> ApiResult<Vec<(PlaySession, String)>> {
    let rows: Vec<(sqlx::types::Json<PlaySession>, String)> = sqlx::query_as(&format!(
        "SELECT {SAVED_STATE},sgf FROM play_sessions ORDER BY updated_at DESC,id DESC LIMIT 100"
    ))
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(rows
        .into_iter()
        .map(|(state, sgf)| (state.0, sgf))
        .collect())
}

pub(super) async fn insert_saved_session(
    pool: &sqlx::PgPool,
    session: &mut PlaySession,
    pinned_path: &str,
) -> ApiResult<()> {
    let sgf = "";
    let native_id = "";
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    library::lock_bot_database(&mut tx).await?;
    sqlx::query(
        "INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(session.id)
    .bind(sqlx::types::Json(&*session))
    .bind(pinned_path)
    .bind(sgf)
    .bind(native_id)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    session.library_match_id = Some(
        library::sync(
            &mut tx,
            session,
            sgf,
            library::ArchiveTarget::NewIn(BOT_DATABASE_ID),
        )
        .await?,
    );
    sqlx::query("UPDATE play_sessions SET state=$2 WHERE id=$1")
        .bind(session.id)
        .bind(sqlx::types::Json(&*session))
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(())
}
