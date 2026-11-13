//! Opt-in background synchronization for a specific local Anki collection.
use super::{
    anki_connect::{self, Client},
    anki_review_sync::{self, Collection},
    *,
};
use std::time::Duration;

const CHECK_INTERVAL: Duration = Duration::from_secs(10);
const SYNC_INTERVAL_SECONDS: i32 = 60;
const SYNC_TIMEOUT: Duration = Duration::from_secs(90);
const CLAIM_SECONDS: i32 = 120;

#[derive(Serialize, ToSchema, Default, sqlx::FromRow)]
pub(super) struct AutoSyncSettings {
    profile: Option<String>,
    last_synced_at: Option<DateTime<Utc>>,
    last_received_at: Option<DateTime<Utc>>,
    last_outcome: Option<SyncOutcome>,
}

#[derive(Clone, Copy, Serialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
enum SyncOutcome {
    Synced,
    NoMatches,
    Failed,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SetAutoSync {
    enabled: bool,
}

#[utoipa::path(get, path="/api/decks/{id}/anki-auto-sync", params(("id"=Uuid,Path)),
    responses((status=200,body=AutoSyncSettings),(status=404,body=ErrorBody)),tag="Learning")]
pub(super) async fn settings(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AutoSyncSettings>> {
    let settings = sqlx::query_as(
        "SELECT s.profile,s.last_synced_at,s.last_received_at,s.last_outcome
         FROM decks d LEFT JOIN anki_auto_sync s ON s.deck_id=d.id WHERE d.id=$1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or_else(ApiError::missing)?;
    Ok(Json(settings))
}

#[utoipa::path(put, path="/api/decks/{id}/anki-auto-sync", params(("id"=Uuid,Path)),request_body=SetAutoSync,
    responses((status=200,body=AutoSyncSettings),(status=404,body=ErrorBody),(status=409,body=ErrorBody),
    (status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=502,body=ErrorBody),
    (status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
pub(super) async fn set_settings(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    LearningJson(input): LearningJson<SetAutoSync>,
) -> ApiResult<Json<AutoSyncSettings>> {
    if !input.enabled {
        require_deck(&state.pool, id).await?;
        sqlx::query("DELETE FROM anki_auto_sync WHERE deck_id=$1")
            .bind(id)
            .execute(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
        return Ok(Json(AutoSyncSettings::default()));
    }

    let _permit = anki_connect::reserve()?;
    let cards = anki_review_sync::deck_cards(&state.pool, id).await?;
    let client = Client::local();
    let collection = Collection::open(&client).await?;
    let key = collection.collection_key(&client).await?;
    if collection.matches(&client, &cards).await?.is_empty() {
        return Err(conflict(
            "Send the deck to the open Anki profile before enabling automatic sync",
        ));
    }
    let settings = sqlx::query_as(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key)
         SELECT id,$2,$3,$4 FROM decks WHERE id=$1
         ON CONFLICT(deck_id) DO UPDATE SET
             link_id=EXCLUDED.link_id,profile=EXCLUDED.profile,collection_key=EXCLUDED.collection_key,
             next_attempt_at=now(),last_synced_at=NULL,last_received_at=NULL,last_outcome=NULL
         RETURNING profile,last_synced_at,last_received_at,last_outcome",
    )
    .bind(id)
    .bind(Uuid::new_v4())
    .bind(collection.profile)
    .bind(key)
    .fetch_optional(&state.pool)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or_else(ApiError::missing)?;
    Ok(Json(settings))
}

/// Checks the active Anki collection only when at least one deck is enabled.
pub async fn worker(state: AppState) {
    let mut tick = tokio::time::interval(CHECK_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = state.shutdown.cancelled() => return,
            _ = tick.tick() => {}
        }
        tokio::select! {
            biased;
            _ = state.shutdown.cancelled() => return,
            result = run_once(&state) => {
                if let Err(error) = result {
                    tracing::debug!(status=%error.0, "Automatic Anki sync check failed");
                }
            }
        }
    }
}

async fn run_once(state: &AppState) -> ApiResult<()> {
    let Ok(_access) = state.backup.requests.try_read() else {
        return Ok(());
    };
    let due: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM anki_auto_sync WHERE next_attempt_at<=now())",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    if !due {
        return Ok(());
    }
    let Ok(_permit) = anki_connect::reserve() else {
        return Ok(());
    };
    let client = Client::local();
    let collection = match Collection::open(&client).await {
        Ok(collection) => collection,
        Err(error) if error.0 == StatusCode::SERVICE_UNAVAILABLE => return Ok(()),
        Err(error) => return Err(error),
    };
    let key = collection.collection_key(&client).await?;
    let Some((deck, link)) = claim(&state.pool, &key).await? else {
        return Ok(());
    };
    let result = tokio::time::timeout(SYNC_TIMEOUT, async {
        let cards = anki_review_sync::deck_cards(&state.pool, deck).await?;
        anki_review_sync::sync_deck(&state.pool, deck, &client, &collection, &cards, Some(link))
            .await
    })
    .await;
    let outcome = match result {
        Ok(Ok(result)) if result.matched_cards == 0 => SyncOutcome::NoMatches,
        Ok(Ok(_)) => SyncOutcome::Synced,
        Ok(Err(error)) => {
            tracing::debug!(%deck, status=%error.0, "Automatic Anki deck sync failed");
            SyncOutcome::Failed
        }
        Err(_) => SyncOutcome::Failed,
    };
    sqlx::query(
        "UPDATE anki_auto_sync SET last_outcome=$3,
            last_synced_at=CASE WHEN $3='synced' THEN now() ELSE last_synced_at END,
            next_attempt_at=now()+make_interval(secs=>$4)
         WHERE deck_id=$1 AND link_id=$2",
    )
    .bind(deck)
    .bind(link)
    .bind(outcome)
    .bind(f64::from(SYNC_INTERVAL_SECONDS))
    .execute(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(())
}

async fn claim(pool: &PgPool, key: &str) -> ApiResult<Option<(Uuid, Uuid)>> {
    Ok(sqlx::query_as(
        "WITH due AS (
            SELECT deck_id FROM anki_auto_sync WHERE collection_key=$1 AND next_attempt_at<=now()
            ORDER BY next_attempt_at,deck_id FOR UPDATE SKIP LOCKED LIMIT 1
         )
         UPDATE anki_auto_sync s SET next_attempt_at=now()+make_interval(secs=>$2)
         FROM due WHERE s.deck_id=due.deck_id RETURNING s.deck_id,s.link_id",
    )
    .bind(key)
    .bind(f64::from(CLAIM_SECONDS))
    .fetch_optional(pool)
    .await
    .map_err(anyhow::Error::from)?)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            claim(&pool, "collection-b").await.map_err(|e| e.1).unwrap(),
            None
        );
        let (first, second) =
            tokio::join!(claim(&pool, "collection-a"), claim(&pool, "collection-a"));
        let claims = [first, second]
            .into_iter()
            .filter_map(|result| result.map_err(|error| error.1).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(claims, [(deck, link)]);
    }
}
