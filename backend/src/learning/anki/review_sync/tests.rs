use super::*;

#[sqlx::test(migrations = "./migrations")]
async fn merges_both_histories_and_retries_without_double_counting(pool: PgPool) {
    let deck = Uuid::new_v4();
    let card = Uuid::new_v4();
    let session = Uuid::new_v4();
    let request = Uuid::new_v4();
    let at = DateTime::from_timestamp_millis(1_760_000_000_000).unwrap();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,'Sync')")
        .bind(deck)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO learning_cards(id,signature,payload,stability,difficulty,last_review,due_at,version,review_count)
         VALUES($1,'fixture','{}',2.0,5.0,$2,$2+interval '2 days',4,1)",
    ).bind(card).bind(at).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO deck_cards VALUES($1,$2)")
        .bind(deck)
        .bind(card)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO review_sessions(id,card_id,card_version,expires_at) VALUES($1,$2,3,$3)",
    )
    .bind(session)
    .bind(card)
    .bind(at)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO card_reviews VALUES($1,$2,$3,'good',$4,$5)")
        .bind(card)
        .bind(request)
        .bind(session)
        .bind(json!({"interval_seconds":172800,"difficulty":5.0}))
        .bind(at)
        .execute(&pool)
        .await
        .unwrap();

    let matched = HashMap::from([(card, "123".into())]);
    let review = |id: i64, kind, factor| RemoteReview {
        id: id.to_string(),
        card_id: "123".into(),
        rating: Some(Rating::Hard),
        interval: Interval::from_seconds(86400).unwrap(),
        previous_interval: Interval::from_seconds(0).unwrap(),
        factor: Factor { value: factor },
        elapsed_ms: 3000,
        kind,
    };
    let occupied = at.timestamp_millis();
    let mut remote = HashMap::from([
        (occupied, review(occupied, Kind::Review, 2500)),
        (occupied - 1, review(occupied - 1, Kind::Manual, 0)),
        (occupied - 2, review(occupied - 2, Kind::Filtered, 0)),
    ]);
    let (received, outgoing) = reconcile(&pool, deck, &matched, &remote, None)
        .await
        .map_err(|e| (e.0, e.1))
        .unwrap();
    assert_eq!(received, 1);
    assert_eq!(outgoing.len(), 1);
    assert_ne!(outgoing[0].id, occupied.to_string());
    assert_eq!(outgoing[0].rating, Rating::Good);

    // A retry after a lost response reuses the original reservation.
    let (received, retry) = reconcile(&pool, deck, &matched, &remote, None)
        .await
        .map_err(|e| (e.0, e.1))
        .unwrap();
    assert_eq!(received, 0);
    assert_eq!(retry[0].id, outgoing[0].id);

    let row: RemoteReview =
        serde_json::from_value(serde_json::to_value(&retry[0]).unwrap()).unwrap();
    remote.insert(row.id.parse().unwrap(), row);
    let (received, outgoing) = reconcile(&pool, deck, &matched, &remote, None)
        .await
        .map_err(|e| (e.0, e.1))
        .unwrap();
    assert_eq!(received, 0);
    assert!(outgoing.is_empty());

    #[derive(FromRow)]
    struct Schedule {
        version: i64,
        due_at: DateTime<Utc>,
        review_count: i32,
        anki_review_count: i32,
        stability: Option<f64>,
        difficulty: Option<f64>,
        last_review: Option<DateTime<Utc>>,
    }
    let persisted: Schedule = sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1")
        .bind(card)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(persisted.anki_review_count, 1);
    assert_eq!(persisted.review_count, 1);
    assert_eq!(persisted.version, 4);
    assert_eq!(persisted.last_review, Some(at));
    assert_eq!(persisted.due_at, at + chrono::Duration::days(2));
    assert_eq!(persisted.stability, Some(2.0));
    assert_eq!(persisted.difficulty, Some(5.0));
}
