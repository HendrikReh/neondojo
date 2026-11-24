use super::*;

pub(crate) async fn require_review_available(pool: &sqlx::PgPool, match_id: Uuid) -> ApiResult<()> {
    require_review_available_on(pool, match_id).await
}

/// Whether a session's result is final: the match is over, or a money game is.
/// SQL reads the same rule from `finished_session_sql!`.
pub(super) fn finished(session: &PlaySession) -> bool {
    matches!(session.phase, PlayPhase::MatchOver) || super::reports::money_finished(session)
}

/// SQL for [`finished`], given the alias of a `play_sessions` row.
macro_rules! finished_session_sql {
    ($session:literal) => {
        concat!(
            "(",
            $session,
            ".state->>'phase'='match_over' OR (",
            $session,
            ".state->>'match_length'='0' AND ",
            $session,
            ".state->>'phase'='game_over'))"
        )
    };
}
pub(crate) use finished_session_sql;

/// SQL for a session that must not be reviewed yet: an unfinished Competition
/// match. Sessions saved before play modes existed count as Competition.
/// The views of the migrations repeat this rule; a test compares them.
macro_rules! active_competition_sql {
    ($session:literal) => {
        concat!(
            "(coalesce(",
            $session,
            ".state->>'play_mode','competition')='competition' AND NOT ",
            $crate::play::finished_session_sql!($session),
            ")"
        )
    };
}
pub(crate) use active_competition_sql;

/// SQL for whether a library match, given its id expression, belongs to an
/// unfinished Competition match.
macro_rules! review_blocked_sql {
    ($match_id:literal) => {
        concat!(
            "EXISTS(SELECT 1 FROM play_sessions live WHERE live.library_match_id=",
            $match_id,
            " AND ",
            $crate::play::active_competition_sql!("live"),
            ")"
        )
    };
}
pub(crate) use review_blocked_sql;

pub(crate) async fn require_review_available_on(
    executor: impl sqlx::PgExecutor<'_>,
    match_id: Uuid,
) -> ApiResult<()> {
    let active: bool = sqlx::query_scalar(concat!("SELECT ", review_blocked_sql!("$1")))
        .bind(match_id)
        .fetch_one(executor)
        .await
        .map_err(anyhow::Error::from)?;
    if active {
        return Err(ApiError::invalid(
            "Complete the competition match before requesting analysis",
        ));
    }
    Ok(())
}

pub(crate) async fn require_position_review_available(
    pool: &sqlx::PgPool,
    position_id: Uuid,
) -> ApiResult<()> {
    let match_id: Option<Uuid> = sqlx::query_scalar("SELECT match_uuid FROM positions WHERE id=$1")
        .bind(position_id)
        .fetch_optional(pool)
        .await
        .map_err(anyhow::Error::from)?;
    if let Some(match_id) = match_id {
        require_review_available(pool, match_id).await?;
    }
    Ok(())
}

pub(crate) async fn require_job_review_available(
    pool: &sqlx::PgPool,
    job: &crate::model::Job,
) -> ApiResult<()> {
    if let Some(match_id) = job.match_uuid {
        require_review_available(pool, match_id).await?;
    }
    if let Some(position_id) = job.position_uuid {
        require_position_review_available(pool, position_id).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const SOURCE: &str = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])\n(;GM[6]FF[4]MI[length:7][ws:1][bs:0];W[31hefe];B[42qusu])";

    fn state(play_mode: Option<&str>, phase: &str, match_length: i32) -> Value {
        let mut state = json!({
            "id": Uuid::new_v4(), "version": 0, "human_player": 0, "human_name": "Human",
            "match_length": match_length, "bearoff_direction": "right",
            "engine": {"id": "gnubg", "version": "test", "plies": 1},
            "position": {"id": Uuid::new_v4(), "match_uuid": Uuid::new_v4(), "ordinal": 0,
                "game_no": 1, "ply": 0, "player": 0, "dice": [3, 1], "board": vec![0; 26],
                "position_id": "", "gnubg_match_id": "", "notation": ""},
            "phase": phase, "can_double": false, "legal_moves": [], "history": [],
            "scores": [0, 0], "game_no": 1, "winner": null,
        });
        if let Some(mode) = play_mode {
            state["play_mode"] = json!(mode);
        }
        state
    }

    /// A match with an analysis of each kind `analysis_catalog_sources` lists.
    async fn analyzed_match(pool: &sqlx::PgPool) -> Uuid {
        let id = crate::store::import_match(
            pool,
            "bot.sgf",
            "sgf",
            SOURCE.as_bytes(),
            SOURCE,
            &crate::import::parse_sgf(SOURCE).unwrap(),
        )
        .await
        .unwrap()
        .r#match
        .id;
        let position: Uuid =
            sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 AND ordinal=0")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap();
        sqlx::query("INSERT INTO imported_analyses(match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) VALUES($1,$2,0,1,1,'checker',$3)")
            .bind(id)
            .bind(position)
            .bind(json!({"candidates": [{"equity": 0.1}]}))
            .execute(pool)
            .await
            .unwrap();
        let analysis = crate::store::create_job(pool, position, "analysis", 1)
            .await
            .unwrap();
        let game = crate::store::create_match_job_scoped(pool, id, 1, 1, Some(1))
            .await
            .unwrap();
        sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES($1,1,$2)")
            .bind(game.id)
            .bind(json!({"completed_games": 1, "total_games": 1}))
            .execute(pool)
            .await
            .unwrap();
        for (job, result) in [
            (analysis.id, json!({"candidates": [{"rank": 1}]})),
            (game.id, json!({})),
        ] {
            sqlx::query("UPDATE jobs SET status='completed',result=$2 WHERE id=$1")
                .bind(job)
                .bind(result)
                .execute(pool)
                .await
                .unwrap();
        }
        id
    }

    /// The migrations' views keep their own copy of the rule, because an applied
    /// migration cannot change.
    #[sqlx::test(migrations = "./migrations")]
    async fn rust_sql_and_views_agree_for_every_session_state(pool: sqlx::PgPool) {
        let id = analyzed_match(&pool).await;
        sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id) VALUES($1,'{}','','','',$2)")
            .bind(Uuid::new_v4())
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let modes = [
            None,
            Some("competition"),
            Some("training"),
            Some("coach"),
            Some("tutor"),
        ];
        let phases = [
            "opening_roll",
            "opening_result",
            "roll",
            "move",
            "double_response",
            "game_over",
            "match_over",
        ];
        for mode in modes {
            for phase in phases {
                for match_length in [0, 7] {
                    let case = format!("{mode:?} {phase} {match_length}");
                    let state = state(mode, phase, match_length);
                    let session: PlaySession = serde_json::from_value(state.clone()).unwrap();
                    let finished = finished(&session);
                    assert_eq!(
                        finished,
                        phase == "match_over" || (match_length == 0 && phase == "game_over"),
                        "{case}"
                    );
                    let active = session.play_mode == PlayMode::Competition && !finished;
                    sqlx::query("UPDATE play_sessions SET state=$1")
                        .bind(&state)
                        .execute(&pool)
                        .await
                        .unwrap();
                    sqlx::query("UPDATE matches SET play_mode=$2,play_human_side=0 WHERE id=$1")
                        .bind(id)
                        .bind(session.play_mode)
                        .execute(&pool)
                        .await
                        .unwrap();

                    let sql: (bool, bool, bool) = sqlx::query_as(concat!(
                        "SELECT ",
                        finished_session_sql!("ps"),
                        ",",
                        active_competition_sql!("ps"),
                        ",",
                        review_blocked_sql!("$1"),
                        " FROM play_sessions ps"
                    ))
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
                    assert_eq!(sql, (finished, active, active), "{case}");
                    assert_eq!(
                        require_review_available(&pool, id).await.is_err(),
                        active,
                        "{case}"
                    );

                    let sources: Vec<(String, String)> = sqlx::query_as(
                        "SELECT source,scope FROM analysis_catalog_sources WHERE match_id=$1 ORDER BY 1,2",
                    )
                    .bind(id)
                    .fetch_all(&pool)
                    .await
                    .unwrap();
                    assert_eq!(
                        sources.len(),
                        if active { 0 } else { 3 },
                        "{case} {sources:?}"
                    );
                    let rated: Vec<bool> = sqlx::query_scalar(
                        "SELECT rated FROM player_match_sides WHERE match_id=$1",
                    )
                    .bind(id)
                    .fetch_all(&pool)
                    .await
                    .unwrap();
                    assert_eq!(
                        rated,
                        [session.play_mode == PlayMode::Competition && finished],
                        "{case}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_rule_is_written_once() {
        fn scan(dir: &std::path::Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    scan(&path, found);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && std::fs::read_to_string(&path)
                        .unwrap()
                        .contains(concat!("->>'phase'", "='game_over'"))
                {
                    found.push(path.file_name().unwrap().to_string_lossy().into_owned());
                }
            }
        }
        let mut found = Vec::new();
        scan(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut found,
        );
        assert_eq!(
            found,
            ["review_gate.rs"],
            "use finished_session_sql!, active_competition_sql! or review_blocked_sql!"
        );
    }
}
