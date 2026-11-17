//! One person can appear under several names: a bot match records "Hendrik",
//! an online import the site's account name. The player directory groups
//! those names as aliases of one profile. Mistake statistics and the decision
//! search follow that grouping, so a player is chosen once and all of their
//! matches count.

use crate::api::ApiResult;
use crate::player_directory::DISPLAY_NAME;
use sqlx::PgPool;

/// The lowercase match names that belong to the chosen player: the name as
/// given plus every alias of its profile. A name that matches several
/// profiles stays on its own, because merging would mix different people.
pub(crate) async fn names(pool: &PgPool, player: &str) -> ApiResult<Vec<String>> {
    let player = player.trim().to_lowercase();
    let mut names: Vec<String> = sqlx::query_scalar(&format!(
        // A match name decides first. The display name only identifies a
        // profile when no alias carries it, such as an edited first and last
        // name; a merged-away profile keeps its old name but has no aliases.
        "WITH by_alias AS (
             SELECT DISTINCT a.profile_id AS id FROM player_aliases a
             WHERE lower(btrim(a.name))=$1
         ), hit AS (
             SELECT id FROM by_alias
             UNION ALL
             SELECT p.id FROM player_profiles p
             WHERE NOT EXISTS(SELECT 1 FROM by_alias) AND lower({DISPLAY_NAME})=$1
               AND EXISTS(SELECT 1 FROM player_aliases a WHERE a.profile_id=p.id)
         )
         SELECT DISTINCT lower(btrim(a.name)) FROM player_aliases a
         WHERE a.profile_id IN (SELECT id FROM hit) AND (SELECT count(*) FROM hit)=1"
    ))
    .bind(&player)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    if !names.contains(&player) {
        names.push(player);
    }
    names.sort();
    Ok(names)
}

/// Replace each match name by its profile's display name, so the aliases of
/// one profile are offered as a single choice.
pub(super) async fn display_names(pool: &PgPool, names: Vec<String>) -> ApiResult<Vec<String>> {
    let shown: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT DISTINCT coalesce(profile.display_name, n.name)
         FROM unnest($1::text[]) AS n(name)
         LEFT JOIN LATERAL (
             SELECT min({DISPLAY_NAME}) AS display_name
             FROM player_profiles p
             WHERE EXISTS(SELECT 1 FROM player_aliases a
                          WHERE a.profile_id=p.id AND lower(btrim(a.name))=lower(n.name))
             HAVING count(*)=1
         ) profile ON true"
    ))
    .bind(names)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(shown)
}
