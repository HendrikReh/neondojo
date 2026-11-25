use super::*;
use crate::import::sgf::GameContext;
use std::sync::Arc;

#[derive(FromRow)]
struct DecisionRow {
    position_uuid: Option<Uuid>,
    game_no: i32,
    data: serde_json::Value,
}

/// Immutable source and replay shared by every selected game of a saved job.
pub(super) struct MatchCardSource {
    job_id: Uuid,
    match_id: Uuid,
    scope: Option<i32>,
    plies: i32,
    engine: Option<String>,
    players: [String; 2],
    catalog_games: Vec<i32>,
    replay: Result<Vec<GameContext>, String>,
}

impl MatchCardSource {
    pub(super) async fn load(pool: &PgPool, id: Uuid) -> ApiResult<Arc<Self>> {
        let job = store::get_job(pool, id)
            .await?
            .ok_or_else(ApiError::missing)?;
        if job.kind != "match" {
            return Err(ApiError::invalid(
                "job_id must refer to a saved match analysis",
            ));
        }
        let match_id = job.match_uuid.context("Match job lacks match UUID")?;
        #[derive(FromRow)]
        struct Source {
            sgf: String,
            white: String,
            black: String,
            catalog_games: Vec<i32>,
            engine: Option<String>,
        }
        let source = sqlx::query_as::<_, Source>(
            "SELECT m.sgf, m.white, m.black,
                    ARRAY(SELECT game_no FROM match_games WHERE match_uuid=m.id) AS catalog_games,
                    (SELECT engine FROM analysis_catalog_sources WHERE match_id=m.id AND job_id=$2) AS engine
             FROM matches m WHERE m.id=$1 AND NOT m.collection_only",
        )
        .bind(match_id)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(|| {
            ApiError::invalid("The source match was removed; add its saved positions instead")
        })?;
        let replay = tokio::task::spawn_blocking(move || {
            crate::import::sgf::parse_sgf_with_context(&source.sgf).map(|(_, games)| games)
        })
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|error| error.to_string());
        Ok(Arc::new(Self {
            job_id: id,
            match_id,
            scope: job.game_no,
            plies: job.plies,
            engine: source.engine,
            players: [source.white, source.black],
            catalog_games: source.catalog_games,
            replay,
        }))
    }

    pub(super) async fn cards(
        self: &Arc<Self>,
        pool: &PgPool,
        game_no: Option<i32>,
        keep: impl Fn(&Decision) -> bool + Send + 'static,
    ) -> ApiResult<Vec<(i32, Result<CardContent, String>)>> {
        if let Some(game) = game_no {
            if self.scope.is_some_and(|scope| scope != game) {
                return Err(ApiError::invalid("game_no does not match this job's scope"));
            }
            // Older matches may not have a persisted game catalog yet.
            let outside = if self.catalog_games.is_empty() {
                let games = self
                    .replay
                    .as_ref()
                    .map_err(|message| anyhow::anyhow!("{message}"))?;
                !usize::try_from(game).is_ok_and(|number| (1..=games.len()).contains(&number))
            } else {
                !self.catalog_games.contains(&game)
            };
            if outside {
                return Err(ApiError::invalid("game_no is outside this match"));
            }
        }
        let rows = sqlx::query_as::<_, DecisionRow>(
            "SELECT ma.position_uuid,ma.game_no,ma.data FROM move_analyses ma
             JOIN match_analysis_games c ON c.job_id=ma.job_id AND c.game_no=ma.game_no
             WHERE ma.job_id=$1 AND ($2::integer IS NULL OR ma.game_no=$2)
             ORDER BY ma.game_no,ma.decision_no LIMIT 200001",
        )
        .bind(self.job_id)
        .bind(game_no)
        .fetch_all(pool)
        .await
        .map_err(anyhow::Error::from)?;
        if rows.len() > 200000 {
            return Err(ApiError::invalid(
                "Analysis exceeds supported decision count",
            ));
        }
        let engine = self
            .engine
            .clone()
            .ok_or_else(|| ApiError::invalid("The selected analysis is no longer available"))?;
        let source = Arc::clone(self);
        tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let contexts = source
                .replay
                .as_ref()
                .map_err(|message| anyhow::anyhow!("{message}"))?;
            let mut output = Vec::new();
            let mut last_game = 0;
            let mut action = 0;
            for row in rows {
                let d: Decision = serde_json::from_value(row.data)?;
                if row.game_no != last_game {
                    last_game = row.game_no;
                    action = 0;
                }
                let context = contexts
                    .get(usize::try_from(row.game_no - 1)?)
                    .context("Missing original game context")?;
                let (position_id, match_state_id) = context
                    .decision_ids
                    .get(action)
                    .context("Missing original decision state")?;
                if d.played != "no double" {
                    action += 1;
                }
                // Advance through every decision before filtering: cube responses have their own IDs.
                if !keep(&d) || (d.kind == "cube" && !context.cube_enabled) {
                    continue;
                }
                let decision_no = d.decision_no;
                let card = decision_card(
                    row.position_uuid,
                    row.game_no,
                    source.match_id,
                    position_id,
                    match_state_id,
                    source.players.clone(),
                    source.job_id,
                    engine.clone(),
                    source.plies,
                    d,
                )
                .map(|mut card| {
                    card.position.cube_enabled = Some(context.cube_enabled);
                    card
                })
                .map_err(|error| error.to_string());
                output.push((decision_no, card));
            }
            Ok(output)
        })
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|error| ApiError::invalid(error.to_string()))
    }
}
