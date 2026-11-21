use super::*;
use crate::{api, engine::Engine, import, store};
use axum::{body::Body, http::Request};
use sqlx::PgPool;
use tower::ServiceExt;

async fn source(pool: &PgPool) -> (Uuid, Uuid) {
    let sgf = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe])";
    let result = store::import_match(
        pool,
        "race.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let id = result.r#match.id;
    let position: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO saved_positions(position_uuid) VALUES($1)")
        .bind(position)
        .execute(pool)
        .await
        .unwrap();
    (id, position)
}

async fn bookmark(pool: PgPool, position: Uuid, method: &str) -> u16 {
    api::router(api::AppState::new(pool, Engine::new("unused".into())))
        .oneshot(
            Request::builder()
                .method(method)
                .uri(format!("/api/positions/{position}/bookmark"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[sqlx::test(migrations = "./migrations")]
async fn waiting_bookmark_reads_saves_and_new_position_jobs_follow_the_retained_owner(
    pool: PgPool,
) {
    let (source, position) = source(&pool).await;
    let mut deletion = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM matches WHERE id=$1 FOR UPDATE")
        .bind(source)
        .execute(&mut *deletion)
        .await
        .unwrap();
    let mut bookmarks = Vec::new();
    for method in ["GET", "PUT"] {
        bookmarks.push(tokio::spawn(bookmark(pool.clone(), position, method)));
    }
    let submit_pool = pool.clone();
    let mut submitted =
        tokio::spawn(async move { store::create_job(&submit_pool, position, "analysis", 1).await });
    for bookmark in &mut bookmarks {
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), bookmark)
                .await
                .is_err()
        );
    }
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut submitted)
            .await
            .is_err()
    );
    preserve(&mut deletion, source).await.unwrap();
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(source)
        .execute(&mut *deletion)
        .await
        .unwrap();
    deletion.commit().await.unwrap();
    for bookmark in bookmarks {
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(2), bookmark)
                .await
                .unwrap()
                .unwrap(),
            200
        );
    }
    let job = tokio::time::timeout(std::time::Duration::from_secs(2), submitted)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(job.position_uuid, Some(position));
    assert_eq!(job.status, "queued");
    let owner = store::get_position(&pool, position)
        .await
        .unwrap()
        .unwrap()
        .match_uuid;
    assert_ne!(owner, source);
    assert!(
        !store::get_match(&pool, owner)
            .await
            .unwrap()
            .unwrap()
            .source_available
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn waiting_bookmark_removal_only_removes_membership_from_the_retained_position(pool: PgPool) {
    let (source, position) = source(&pool).await;
    let mut deletion = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM matches WHERE id=$1 FOR UPDATE")
        .bind(source)
        .execute(&mut *deletion)
        .await
        .unwrap();
    let mut removal = tokio::spawn(bookmark(pool.clone(), position, "DELETE"));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut removal)
            .await
            .is_err()
    );
    preserve(&mut deletion, source).await.unwrap();
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(source)
        .execute(&mut *deletion)
        .await
        .unwrap();
    deletion.commit().await.unwrap();
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(2), removal)
            .await
            .unwrap()
            .unwrap(),
        204
    );
    assert!(
        store::get_position(&pool, position)
            .await
            .unwrap()
            .is_some()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM saved_positions WHERE position_uuid=$1")
            .bind(position)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_first_game_note_is_preserved_before_the_old_game_cascades(pool: PgPool) {
    let (source, position) = source(&pool).await;
    let game: Uuid =
        sqlx::query_scalar("SELECT id FROM match_games WHERE match_uuid=$1 AND game_no=1")
            .bind(source)
            .fetch_one(&pool)
            .await
            .unwrap();
    // This is the game annotation PUT's target-lock/first-insert sequence, with
    // its first insert paused until preservation has had time to start.
    let mut note = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM match_games WHERE id=$1 FOR KEY SHARE")
        .bind(game)
        .execute(&mut *note)
        .await
        .unwrap();
    let deleting_pool = pool.clone();
    let mut preserving = tokio::spawn(async move {
        let mut tx = deleting_pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM matches WHERE id=$1 FOR UPDATE")
            .bind(source)
            .execute(&mut *tx)
            .await
            .unwrap();
        preserve(&mut tx, source).await.unwrap();
        tx
    });
    let early = tokio::time::timeout(std::time::Duration::from_secs(1), &mut preserving)
        .await
        .ok();
    sqlx::query("INSERT INTO annotations(id,game_uuid,tags,comment,version) VALUES($1,$2,ARRAY['concurrent'],'First game note',1)")
        .bind(Uuid::new_v4()).bind(game).execute(&mut *note).await.unwrap();
    note.commit().await.unwrap();
    let mut deletion = match early {
        Some(tx) => tx.unwrap(),
        None => tokio::time::timeout(std::time::Duration::from_secs(2), preserving)
            .await
            .unwrap()
            .unwrap(),
    };
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(source)
        .execute(&mut *deletion)
        .await
        .unwrap();
    deletion.commit().await.unwrap();
    let retained:Option<String>=sqlx::query_scalar("SELECT a.comment FROM annotations a JOIN match_games g ON g.id=a.game_uuid JOIN positions p ON p.match_uuid=g.match_uuid AND p.game_no=g.game_no WHERE p.id=$1")
        .bind(position).fetch_optional(&pool).await.unwrap();
    assert_eq!(
        retained,
        Some("First game note".into()),
        "a successful first note save must survive source deletion"
    );
}
