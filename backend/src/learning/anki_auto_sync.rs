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
const HISTORY_LIMIT: i64 = 100;

#[derive(Default)]
pub(crate) struct AnkiConnection(tokio::sync::RwLock<ConnectionState>);

#[derive(Default)]
enum ConnectionState {
    #[default]
    Checking,
    Unreachable,
    Timeout,
    AnkiError,
    Open(String),
}

#[derive(Clone, Copy, Serialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
enum SyncProblem {
    Unreachable,
    InvalidDeck,
    Conflict,
    Timeout,
    AnkiError,
    InternalError,
}

impl From<StatusCode> for SyncProblem {
    fn from(status: StatusCode) -> Self {
        match status {
            StatusCode::SERVICE_UNAVAILABLE => Self::Unreachable,
            StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY => Self::InvalidDeck,
            StatusCode::CONFLICT => Self::Conflict,
            StatusCode::GATEWAY_TIMEOUT => Self::Timeout,
            StatusCode::BAD_GATEWAY => Self::AnkiError,
            _ => Self::InternalError,
        }
    }
}

#[derive(Default, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum OverviewStatus {
    #[default]
    Checking,
    Paused,
    Unreachable,
    OtherProfile,
    Waiting,
    Synced,
    NoMatches,
    InvalidDeck,
    Conflict,
    Timeout,
    AnkiError,
    InternalError,
    Failed,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub(super) struct LinkedAnkiDeck {
    deck_id: Uuid,
    deck_name: String,
    profile: String,
    last_synced_at: Option<DateTime<Utc>>,
    received_reviews: i64,
    sent_reviews: i64,
    open_errors: i64,
    paused: bool,
    #[serde(skip)]
    collection_key: String,
    #[serde(skip)]
    last_outcome: Option<SyncOutcome>,
    #[serde(skip)]
    last_problem: Option<SyncProblem>,
    #[sqlx(skip)]
    status: OverviewStatus,
}

impl LinkedAnkiDeck {
    fn status(&self, connection: &ConnectionState) -> OverviewStatus {
        if self.paused {
            return OverviewStatus::Paused;
        }
        match connection {
            ConnectionState::Checking => OverviewStatus::Checking,
            ConnectionState::Unreachable => OverviewStatus::Unreachable,
            ConnectionState::Timeout => OverviewStatus::Timeout,
            ConnectionState::AnkiError => OverviewStatus::AnkiError,
            ConnectionState::Open(key) if key != &self.collection_key => {
                OverviewStatus::OtherProfile
            }
            ConnectionState::Open(_) => match self.last_outcome {
                None => OverviewStatus::Waiting,
                Some(SyncOutcome::Synced) => OverviewStatus::Synced,
                Some(SyncOutcome::NoMatches) => OverviewStatus::NoMatches,
                Some(SyncOutcome::Failed) => match self.last_problem {
                    Some(SyncProblem::Unreachable) => OverviewStatus::Unreachable,
                    Some(SyncProblem::InvalidDeck) => OverviewStatus::InvalidDeck,
                    Some(SyncProblem::Conflict) => OverviewStatus::Conflict,
                    Some(SyncProblem::Timeout) => OverviewStatus::Timeout,
                    Some(SyncProblem::AnkiError) => OverviewStatus::AnkiError,
                    Some(SyncProblem::InternalError) => OverviewStatus::InternalError,
                    None => OverviewStatus::Failed,
                },
            },
        }
    }
}

#[utoipa::path(get, path="/api/study/anki-auto-sync",
    responses((status=200,body=Vec<LinkedAnkiDeck>)),tag="Learning")]
pub(super) async fn overview(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<LinkedAnkiDeck>>> {
    let mut decks: Vec<LinkedAnkiDeck> = sqlx::query_as(
        "SELECT d.id AS deck_id,d.name AS deck_name,s.profile,s.last_synced_at,
                s.received_reviews,s.sent_reviews,s.paused,s.collection_key,s.last_outcome,s.last_problem,
                (SELECT count(*) FROM anki_sync_history h
                 WHERE h.deck_id=s.deck_id AND h.outcome='failed' AND h.resolved_at IS NULL) AS open_errors
         FROM anki_auto_sync s JOIN decks d ON d.id=s.deck_id ORDER BY lower(d.name),d.id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    let connection = state.anki_connection.0.read().await;
    for deck in &mut decks {
        deck.status = deck.status(&connection);
    }
    Ok(Json(decks))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub(super) struct AnkiSyncHistoryEntry {
    id: Uuid,
    profile: String,
    finished_at: DateTime<Utc>,
    received_reviews: i64,
    sent_reviews: i64,
    outcome: SyncOutcome,
    problem: Option<SyncProblem>,
    error_detail: Option<String>,
    resolved_at: Option<DateTime<Utc>>,
}

#[utoipa::path(get, path="/api/decks/{id}/anki-auto-sync/history", params(("id"=Uuid,Path)),
    responses((status=200,body=Vec<AnkiSyncHistoryEntry>),(status=404,body=ErrorBody)),tag="Learning")]
pub(super) async fn history(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<AnkiSyncHistoryEntry>>> {
    let rows: Vec<AnkiSyncHistoryEntry> = sqlx::query_as(
        "SELECT h.id,h.profile,h.finished_at,h.received_reviews,h.sent_reviews,h.outcome,h.problem,h.error_detail,h.resolved_at
         FROM anki_sync_history h JOIN anki_auto_sync s USING (deck_id)
         WHERE h.deck_id=$1 ORDER BY h.finished_at DESC,h.id DESC LIMIT $2",
    )
    .bind(id)
    .bind(HISTORY_LIMIT)
    .fetch_all(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    if rows.is_empty() {
        let linked: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM anki_auto_sync WHERE deck_id=$1)")
                .bind(id)
                .fetch_one(&state.pool)
                .await
                .map_err(anyhow::Error::from)?;
        if !linked {
            return Err(ApiError::missing());
        }
    }
    Ok(Json(rows))
}

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

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum SyncAction {
    SyncNow,
    RetryOpenErrors,
    Pause,
    Resume,
}

#[derive(Clone, Copy)]
enum Retry {
    None,
    Entry(Uuid),
    OpenErrors,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SyncActionRequest {
    action: SyncAction,
    retry_of: Option<Uuid>,
    #[serde(default)]
    include_result: bool,
}

#[derive(Serialize, ToSchema)]
pub(super) struct SyncAttemptResult {
    outcome: SyncOutcome,
    received_reviews: i64,
    sent_reviews: i64,
    open_errors: i64,
    error_detail: Option<String>,
}

struct FinishedSync {
    result: SyncAttemptResult,
    error: Option<ApiError>,
}

impl FinishedSync {
    fn into_status(self) -> ApiResult<()> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[utoipa::path(post, path="/api/decks/{id}/anki-auto-sync/action", params(("id"=Uuid,Path)),
    request_body=SyncActionRequest,
    responses((status=200,body=SyncAttemptResult),(status=204),(status=404,body=ErrorBody),(status=409,body=ErrorBody),
    (status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=502,body=ErrorBody),
    (status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
pub(super) async fn act(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    crate::json_errors::Json(input): crate::json_errors::Json<SyncActionRequest>,
) -> ApiResult<axum::response::Response> {
    if input.include_result && !matches!(input.action, SyncAction::RetryOpenErrors) {
        return Err(ApiError::invalid(
            "A result can be requested when retrying open errors",
        ));
    }
    match input.action {
        SyncAction::SyncNow => {
            sync_now(&state, id, input.retry_of.map_or(Retry::None, Retry::Entry))
                .await?
                .into_status()?
        }
        _ if input.retry_of.is_some() => {
            return Err(ApiError::invalid("Retry requires a review sync"));
        }
        SyncAction::RetryOpenErrors => {
            let finished = sync_now(&state, id, Retry::OpenErrors).await?;
            if input.include_result {
                return Ok(Json(finished.result).into_response());
            }
            finished.into_status()?;
        }
        SyncAction::Pause => set_paused(&state.pool, id, true).await?,
        SyncAction::Resume => set_paused(&state.pool, id, false).await?,
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn set_paused(pool: &PgPool, id: Uuid, paused: bool) -> ApiResult<()> {
    let result = sqlx::query(
        "UPDATE anki_auto_sync SET paused=$2,
            next_attempt_at=CASE WHEN paused AND NOT $2 THEN now() ELSE next_attempt_at END
         WHERE deck_id=$1",
    )
    .bind(id)
    .bind(paused)
    .execute(pool)
    .await
    .map_err(anyhow::Error::from)?;
    if result.rows_affected() == 0 {
        return Err(ApiError::missing());
    }
    Ok(())
}

async fn sync_now(state: &AppState, id: Uuid, retry: Retry) -> ApiResult<FinishedSync> {
    let _permit = anki_connect::reserve()?;
    let key: String =
        sqlx::query_scalar("SELECT collection_key FROM anki_auto_sync WHERE deck_id=$1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .map_err(anyhow::Error::from)?
            .ok_or_else(ApiError::missing)?;
    let Some(claimed) = claim(&state.pool, &key, Some(id), retry).await? else {
        if !matches!(retry, Retry::None) {
            return Err(conflict(
                "The error is no longer open for this deck or a sync is already running",
            ));
        }
        return Err(conflict(
            "The link changed or this deck is already synchronizing",
        ));
    };
    let client = Client::local();
    let collection = probe_collection(state, &client)
        .await
        .and_then(|(collection, open_key)| {
            if open_key == key {
                Ok(collection)
            } else {
                Err(conflict("Open the linked Anki profile"))
            }
        });
    run_claimed(&state.pool, claimed, &client, collection).await
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
    crate::json_errors::Json(input): crate::json_errors::Json<SetAutoSync>,
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
    let updated = sqlx::query_as(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key)
         SELECT id,$2,$3,$4 FROM decks WHERE id=$1
         ON CONFLICT(deck_id) DO UPDATE SET
             link_id=EXCLUDED.link_id,profile=EXCLUDED.profile,collection_key=EXCLUDED.collection_key,
             next_attempt_at=now(),last_synced_at=NULL,last_received_at=NULL,last_outcome=NULL,
             last_problem=NULL,received_reviews=0,sent_reviews=0,paused=false,running_until=NULL
         WHERE anki_auto_sync.collection_key <> EXCLUDED.collection_key
         RETURNING profile,last_synced_at,last_received_at,last_outcome",
    )
    .bind(id)
    .bind(Uuid::new_v4())
    .bind(collection.profile)
    .bind(key)
    .fetch_optional(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    match updated {
        Some(updated) => Ok(Json(updated)),
        None => settings(State(state), Path(id)).await,
    }
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
    let linked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM anki_auto_sync)")
        .fetch_one(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
    if !linked {
        *state.anki_connection.0.write().await = ConnectionState::Checking;
        return Ok(());
    }
    let client = Client::local();
    let Ok((collection, key)) = probe_collection(state, &client).await else {
        return Ok(());
    };
    let Ok(_permit) = anki_connect::reserve() else {
        return Ok(());
    };
    let Some(claimed) = claim(&state.pool, &key, None, Retry::None).await? else {
        return Ok(());
    };
    run_claimed(&state.pool, claimed, &client, Ok(collection))
        .await?
        .into_status()
}

async fn probe_collection(state: &AppState, client: &Client) -> ApiResult<(Collection, String)> {
    let probe = async {
        let collection = Collection::open(client).await?;
        let key = collection.collection_key(client).await?;
        Ok::<_, ApiError>((collection, key))
    };
    let result = tokio::time::timeout(CHECK_INTERVAL, probe)
        .await
        .unwrap_or_else(|_| {
            Err(ApiError(
                StatusCode::GATEWAY_TIMEOUT,
                "AnkiConnect did not respond in time".into(),
            ))
        });
    let status = match &result {
        Ok((_, key)) => ConnectionState::Open(key.clone()),
        Err(error) if error.0 == StatusCode::SERVICE_UNAVAILABLE => ConnectionState::Unreachable,
        Err(error) if error.0 == StatusCode::GATEWAY_TIMEOUT => ConnectionState::Timeout,
        Err(_) => ConnectionState::AnkiError,
    };
    *state.anki_connection.0.write().await = status;
    result
}

async fn run_claimed(
    pool: &PgPool,
    claimed: ClaimedSync,
    client: &Client,
    collection: ApiResult<Collection>,
) -> ApiResult<FinishedSync> {
    let result = tokio::time::timeout(SYNC_TIMEOUT, async {
        let collection = collection?;
        let cards = anki_review_sync::deck_cards(pool, claimed.deck_id).await?;
        anki_review_sync::sync_deck(
            pool,
            claimed.deck_id,
            client,
            &collection,
            &cards,
            Some(claimed.link_id),
        )
        .await
    })
    .await
    .unwrap_or_else(|_| {
        Err(ApiError(
            StatusCode::GATEWAY_TIMEOUT,
            "AnkiConnect did not confirm the sync in time".into(),
        ))
    });
    let (outcome, problem) = match &result {
        Ok(result) if result.matched_cards == 0 => (SyncOutcome::NoMatches, None),
        Ok(_) => (SyncOutcome::Synced, None),
        Err(error) => (SyncOutcome::Failed, Some(SyncProblem::from(error.0))),
    };
    let summary = finish(pool, claimed, outcome, problem, result.as_ref().err()).await?;
    let error = if matches!(outcome, SyncOutcome::NoMatches) {
        Some(conflict("Send the deck to the linked Anki profile"))
    } else {
        result.err()
    };
    Ok(FinishedSync {
        result: summary,
        error,
    })
}

#[derive(Debug, PartialEq, sqlx::FromRow)]
struct ClaimedSync {
    deck_id: Uuid,
    link_id: Uuid,
    running_until: DateTime<Utc>,
    received_reviews: i64,
    sent_reviews: i64,
    retry_of: Option<Uuid>,
    retry_open_errors: bool,
}

// The claim excludes other tracked runs for this link. Subtracting its starting
// totals includes every confirmed batch even when a later operation fails.
async fn finish(
    pool: &PgPool,
    claimed: ClaimedSync,
    outcome: SyncOutcome,
    problem: Option<SyncProblem>,
    error: Option<&ApiError>,
) -> ApiResult<SyncAttemptResult> {
    let detail = error.map(|error| {
        error
            .1
            .chars()
            .filter(|c| !c.is_control())
            .take(300)
            .collect::<String>()
    });
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    let attempt: Option<(DateTime<Utc>, i64, i64)> = sqlx::query_as(
        "WITH finished AS (
            UPDATE anki_auto_sync SET last_outcome=$4,
                last_synced_at=CASE WHEN $4='synced' THEN now() ELSE last_synced_at END,
                next_attempt_at=now()+make_interval(secs=>$5),last_problem=$6,running_until=NULL
             WHERE deck_id=$1 AND link_id=$2 AND running_until=$3
             RETURNING profile,received_reviews,sent_reviews
         )
         INSERT INTO anki_sync_history(deck_id,profile,received_reviews,sent_reviews,outcome,problem,error_detail)
         SELECT $1,profile,received_reviews-$7,sent_reviews-$8,$4,$6,$9 FROM finished
         RETURNING finished_at,received_reviews,sent_reviews",
    )
    .bind(claimed.deck_id)
    .bind(claimed.link_id)
    .bind(claimed.running_until)
    .bind(outcome)
    .bind(f64::from(SYNC_INTERVAL_SECONDS))
    .bind(problem)
    .bind(claimed.received_reviews)
    .bind(claimed.sent_reviews)
    .bind(&detail)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let Some((finished_at, received_reviews, sent_reviews)) = attempt else {
        return Err(conflict("The Anki link changed during synchronization"));
    };
    if matches!(outcome, SyncOutcome::Synced)
        && (claimed.retry_open_errors || claimed.retry_of.is_some())
    {
        sqlx::query(
            "UPDATE anki_sync_history SET resolved_at=$2
             WHERE deck_id=$1 AND outcome='failed' AND resolved_at IS NULL
             AND ($3 OR id=$4)",
        )
        .bind(claimed.deck_id)
        .bind(finished_at)
        .bind(claimed.retry_open_errors)
        .bind(claimed.retry_of)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    }
    sqlx::query(
        "DELETE FROM anki_sync_history WHERE deck_id=$1 AND id IN (
            SELECT id FROM anki_sync_history WHERE deck_id=$1
            ORDER BY finished_at DESC,id DESC OFFSET $2
         )",
    )
    .bind(claimed.deck_id)
    .bind(HISTORY_LIMIT)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let open_errors = sqlx::query_scalar(
        "SELECT count(*) FROM anki_sync_history
         WHERE deck_id=$1 AND outcome='failed' AND resolved_at IS NULL",
    )
    .bind(claimed.deck_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(SyncAttemptResult {
        outcome,
        received_reviews,
        sent_reviews,
        open_errors,
        error_detail: detail,
    })
}

async fn claim(
    pool: &PgPool,
    key: &str,
    requested: Option<Uuid>,
    retry: Retry,
) -> ApiResult<Option<ClaimedSync>> {
    let retry_of = match retry {
        Retry::Entry(id) => Some(id),
        _ => None,
    };
    Ok(sqlx::query_as(
        "WITH due AS (
            SELECT deck_id FROM anki_auto_sync s WHERE collection_key=$1
            AND (running_until IS NULL OR running_until<=now())
            AND CASE WHEN $3::uuid IS NULL THEN NOT paused AND next_attempt_at<=now()
                     ELSE deck_id=$3 END
            AND (($4::uuid IS NULL AND NOT $5::boolean) OR EXISTS (
                SELECT 1 FROM anki_sync_history h WHERE h.deck_id=s.deck_id
                AND ($5 OR h.id=$4) AND h.outcome='failed' AND h.resolved_at IS NULL
            ))
            ORDER BY next_attempt_at,deck_id FOR UPDATE SKIP LOCKED LIMIT 1
         )
         UPDATE anki_auto_sync s SET running_until=now()+make_interval(secs=>$2)
         FROM due WHERE s.deck_id=due.deck_id
         RETURNING s.deck_id,s.link_id,s.running_until,s.received_reviews,s.sent_reviews,
                   $4 AS retry_of,$5 AS retry_open_errors",
    )
    .bind(key)
    .bind(f64::from(CLAIM_SECONDS))
    .bind(requested)
    .bind(retry_of)
    .bind(matches!(retry, Retry::OpenErrors))
    .fetch_optional(pool)
    .await
    .map_err(anyhow::Error::from)?)
}

#[cfg(test)]
mod tests {
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
        let untouched: bool = sqlx::query_scalar(
            "SELECT resolved_at IS NULL FROM anki_sync_history WHERE deck_id=$1",
        )
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
}
