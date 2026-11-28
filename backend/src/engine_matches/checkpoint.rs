use crate::play::engine_match::{GameCheckpoint, SavedGame};
use anyhow::{Context, Result, ensure};
use futures_util::TryStreamExt;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

pub(super) async fn load<'e>(
    db: impl PgExecutor<'e>,
    id: Uuid,
    index: i32,
) -> Result<Option<SavedGame>> {
    let mut rows = sqlx::query_as::<_, (i32, i32, String, Vec<u8>)>(
        "SELECT game_number,sgf_prefix_bytes,sgf_suffix,state FROM engine_match_game_checkpoints
         WHERE run_id=$1 AND unit_index=$2 ORDER BY game_number",
    )
    .bind(id)
    .bind(index)
    .fetch(db);
    let mut latest = None;
    let mut sgf = String::new();
    let mut evaluations = Vec::new();
    let mut last_game = 0;
    while let Some((game, prefix, suffix, payload)) = rows.try_next().await? {
        let prefix = usize::try_from(prefix)?;
        ensure!(
            game == last_game + 1 && sgf.is_char_boundary(prefix),
            "Invalid duel game checkpoint sequence"
        );
        sgf.truncate(prefix);
        sgf.push_str(&suffix);
        let mut state: GameCheckpoint =
            serde_smile::from_slice(&payload).context("Invalid saved duel game state")?;
        ensure!(
            state.progress.completed_games == game,
            "Invalid saved duel game number"
        );
        evaluations.append(&mut state.evaluations);
        latest = Some(state);
        last_game = game;
    }
    Ok(latest.map(|mut state| {
        state.evaluations = evaluations;
        SavedGame { sgf, state }
    }))
}

pub(super) fn save<'a>(
    pool: &'a PgPool,
    id: Uuid,
    token: Uuid,
    index: i32,
    previous_sgf: &str,
    sgf: &str,
    state: &GameCheckpoint,
) -> impl Future<Output = Result<()>> + use<'a> {
    let (prefix, suffix) = sgf_delta(previous_sgf, sgf);
    let suffix = suffix.to_owned();
    let payload = serde_smile::to_vec(state);
    let game = state.progress.completed_games;
    async move {
        let result = sqlx::query(
            "WITH owned AS MATERIALIZED (
                SELECT id,lease_until FROM engine_match_runs
                WHERE id=$1 AND lease_token=$2 AND status='running' AND completed_matches=$3 FOR UPDATE
             )
             INSERT INTO engine_match_game_checkpoints (run_id,unit_index,game_number,sgf_prefix_bytes,sgf_suffix,state)
             SELECT id,$3,$4,$5,$6,$7 FROM owned WHERE lease_until>clock_timestamp()",
        )
        .bind(id).bind(token).bind(index).bind(game).bind(i32::try_from(prefix)?).bind(suffix).bind(payload?)
        .execute(pool).await?;
        ensure!(
            result.rows_affected() == 1,
            "Duel game checkpoint lost its lease"
        );
        Ok(())
    }
}

/// GNUbg normally appends one game to the previous SGF. If it rewrites a
/// previous property, keep the changed tail so the restored SGF is exact.
fn sgf_delta<'a>(previous: &str, current: &'a str) -> (usize, &'a str) {
    let trimmed = previous.trim_end();
    if let Some(suffix) = current.strip_prefix(trimmed) {
        return (trimmed.len(), suffix);
    }
    let mut prefix = previous
        .bytes()
        .zip(current.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !current.is_char_boundary(prefix) {
        prefix -= 1;
    }
    (prefix, &current[prefix..])
}

#[cfg(test)]
mod tests {
    use super::sgf_delta;

    #[test]
    fn sgf_delta_stores_only_the_changed_tail_and_splits_at_a_character_boundary() {
        for (before, after, suffix) in [
            ("(;GM[6])\n", "(;GM[6])\n(;GM[6])\n", "\n(;GM[6])\n"),
            ("(;C[ä])", "(;C[ö])", "ö])"),
            ("", "(;GM[6])", "(;GM[6])"),
        ] {
            let (prefix, delta) = sgf_delta(before, after);
            assert_eq!(delta, suffix);
            assert_eq!(format!("{}{delta}", &before[..prefix]), after);
        }
    }
}
