use super::*;

#[sqlx::test(migrations = "./migrations")]
async fn successful_retry_resolves_only_the_selected_error(pool: PgPool) {
    let deck = Uuid::new_v4();
    let target = Uuid::new_v4();
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,'Linked')")
        .bind(deck)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key,paused)
                 VALUES($1,gen_random_uuid(),'Profile','key',true)",
    )
    .bind(deck)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO anki_sync_history(id,deck_id,profile,received_reviews,sent_reviews,outcome,problem,error_detail)
                 SELECT id,$1,'Profile',2,1,'failed','timeout','Confirmation timed out'
                 FROM unnest($2::uuid[]) AS id")
        .bind(deck).bind([target, other]).execute(&pool).await.unwrap();

    for (retry_of, outcome) in [
        (None, SyncOutcome::Synced),
        (Some(target), SyncOutcome::NoMatches),
        (Some(target), SyncOutcome::Synced),
    ] {
        let claimed = claim(
            &pool,
            "key",
            Some(deck),
            retry_of.map_or(Retry::None, Retry::Entry),
        )
        .await
        .map_err(|err| err.1)
        .unwrap()
        .unwrap();
        finish(&pool, claimed, outcome, None, None)
            .await
            .map_err(|err| err.1)
            .unwrap();
        let originals: Vec<(Uuid, bool, i64, i64, String)> = sqlx::query_as(
            "SELECT id,resolved_at IS NOT NULL,received_reviews,sent_reviews,error_detail
             FROM anki_sync_history WHERE id=ANY($1)",
        )
        .bind([target, other])
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(originals.len(), 2);
        for (id, resolved, received, sent, error) in originals {
            assert_eq!(
                resolved,
                id == target && retry_of.is_some() && matches!(outcome, SyncOutcome::Synced)
            );
            assert_eq!(
                (received, sent, error.as_str()),
                (2, 1, "Confirmation timed out")
            );
        }
    }
    assert!(
        claim(&pool, "key", Some(deck), Retry::Entry(target))
            .await
            .map_err(|err| err.1)
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn retry_open_errors_resolves_the_deck_only_after_success(pool: PgPool) {
    let deck = Uuid::new_v4();
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,'Linked'),($2,'Other')")
        .bind(deck)
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key,paused)
                 VALUES($1,gen_random_uuid(),'Profile','key',true)",
    )
    .bind(deck)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO anki_sync_history(deck_id,profile,received_reviews,sent_reviews,outcome,problem,error_detail)
                 VALUES($1,'Profile',2,1,'failed','timeout','Timed out'),
                       ($1,'Profile',0,0,'failed','conflict','Profile changed'),
                       ($2,'Profile',0,0,'failed','timeout','Other failure')")
        .bind(deck).bind(other).execute(&pool).await.unwrap();
    let error = ApiError(StatusCode::GATEWAY_TIMEOUT, "Confirmation timed out".into());
    for (outcome, problem, error, expected_open) in [
        (
            SyncOutcome::Failed,
            Some(SyncProblem::Timeout),
            Some(&error),
            3,
        ),
        (SyncOutcome::NoMatches, None, None, 3),
        (SyncOutcome::Synced, None, None, 0),
    ] {
        let claimed = claim(&pool, "key", Some(deck), Retry::OpenErrors)
            .await
            .map_err(|err| err.1)
            .unwrap()
            .unwrap();
        let result = finish(&pool, claimed, outcome, problem, error)
            .await
            .map_err(|err| err.1)
            .unwrap();
        assert_eq!(result.open_errors, expected_open);
        let open: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM anki_sync_history
             WHERE deck_id=$1 AND outcome='failed' AND resolved_at IS NULL",
        )
        .bind(deck)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(open, expected_open);
    }
    let untouched: bool =
        sqlx::query_scalar("SELECT resolved_at IS NULL FROM anki_sync_history WHERE deck_id=$1")
            .bind(other)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(untouched);
    let original: (i64, i64, bool) = sqlx::query_as(
        "SELECT received_reviews,sent_reviews,resolved_at IS NOT NULL
         FROM anki_sync_history WHERE deck_id=$1 AND error_detail='Timed out'",
    )
    .bind(deck)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(original, (2, 1, true));
    assert!(
        claim(&pool, "key", Some(deck), Retry::OpenErrors)
            .await
            .map_err(|err| err.1)
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn failed_run_keeps_confirmed_counts_and_retry_only_records_new_transfers(pool: PgPool) {
    let deck = Uuid::new_v4();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,'Linked')")
        .bind(deck)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key,received_reviews,sent_reviews)
         VALUES($1,$2,'Profile','key',4,8)",
    )
    .bind(deck)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();
    let first = claim(&pool, "key", Some(deck), Retry::None)
        .await
        .map_err(|err| err.1)
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE anki_auto_sync SET received_reviews=6,sent_reviews=9 WHERE deck_id=$1")
        .bind(deck)
        .execute(&pool)
        .await
        .unwrap();
    let error = ApiError(StatusCode::GATEWAY_TIMEOUT, "Confirmation timed out".into());
    let failed = finish(
        &pool,
        first,
        SyncOutcome::Failed,
        Some(SyncProblem::Timeout),
        Some(&error),
    )
    .await
    .map_err(|err| err.1)
    .unwrap();
    assert_eq!(
        (
            failed.received_reviews,
            failed.sent_reviews,
            failed.open_errors
        ),
        (2, 1, 1)
    );
    let second = claim(&pool, "key", Some(deck), Retry::None)
        .await
        .map_err(|err| err.1)
        .unwrap()
        .unwrap();
    let retried = finish(&pool, second, SyncOutcome::Synced, None, None)
        .await
        .map_err(|err| err.1)
        .unwrap();
    assert_eq!((retried.received_reviews, retried.sent_reviews), (0, 0));
    let rows: Vec<(i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT received_reviews,sent_reviews,error_detail FROM anki_sync_history
         WHERE deck_id=$1 ORDER BY finished_at DESC,id DESC",
    )
    .bind(deck)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        [(0, 0, None), (2, 1, Some("Confirmation timed out".into()))]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn overview_keeps_transfer_totals_visible_when_connection_pauses(pool: PgPool) {
    let deck = Uuid::new_v4();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,'Linked')")
        .bind(deck)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key,last_synced_at,
              received_reviews,sent_reviews,last_outcome,last_problem)
         VALUES($1,$2,'Profile','key',now(),2,3,'failed','timeout')",
    )
    .bind(deck)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();
    let state = AppState::new(pool, crate::engine::Engine::new("unused".into()));
    for (connection, status) in [
        (ConnectionState::Unreachable, "unreachable"),
        (ConnectionState::Open("other".into()), "other_profile"),
        (ConnectionState::Open("key".into()), "timeout"),
    ] {
        *state.anki_connection.0.write().await = connection;
        let Json(rows) = overview(State(state.clone()))
            .await
            .map_err(|error| error.1)
            .unwrap();
        assert_eq!(rows.len(), 1);
        let row = serde_json::to_value(&rows[0]).unwrap();
        assert_eq!(row["status"], status);
        assert_eq!(row["received_reviews"], 2);
        assert_eq!(row["sent_reviews"], 3);
        assert!(row["last_synced_at"].is_string());
        assert!(row.get("collection_key").is_none());
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn automatic_claims_are_collection_scoped_and_exclusive(pool: PgPool) {
    let deck = Uuid::new_v4();
    let link = Uuid::new_v4();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,'Linked')")
        .bind(deck)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key)
         VALUES($1,$2,'Same profile name','collection-a')",
    )
    .bind(deck)
    .bind(link)
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        claim(&pool, "collection-b", None, Retry::None)
            .await
            .map_err(|e| e.1)
            .unwrap(),
        None
    );
    let (first, second) = tokio::join!(
        claim(&pool, "collection-a", None, Retry::None),
        claim(&pool, "collection-a", None, Retry::None)
    );
    let claims = [first, second]
        .into_iter()
        .filter_map(|result| result.map_err(|error| error.1).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(claims.len(), 1);
    assert_eq!((claims[0].deck_id, claims[0].link_id), (deck, link));
    assert!(
        claim(&pool, "collection-a", Some(deck), Retry::None)
            .await
            .map_err(|e| e.1)
            .unwrap()
            .is_none()
    );
    set_paused(&pool, deck, true)
        .await
        .map_err(|e| e.1)
        .unwrap();
    sqlx::query("UPDATE anki_auto_sync SET running_until=now()-interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        claim(&pool, "collection-a", None, Retry::None)
            .await
            .map_err(|e| e.1)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        claim(&pool, "collection-a", Some(deck), Retry::None)
            .await
            .map_err(|e| e.1)
            .unwrap()
            .map(|claimed| (claimed.deck_id, claimed.link_id)),
        Some((deck, link))
    );
}
