//! One person can appear under several names: a bot match records "Hendrik",
//! an online import the site's account name. The player directory groups
//! those names as aliases of one profile, and a virtual profile groups real
//! profiles, such as the same person's accounts on several sites. This module
//! is the one place that decides which matches and names belong to a chosen
//! player; mistake statistics, the decision search, transfer practice, the
//! player directory and the library follow it.
//!
//! NeonDojo bot matches catalogue their human side like an import, under the
//! NeonDojo platform. The views `player_match_sides` and `virtual_player_sides`
//! define a real profile's sides and a virtual profile's members' sides, and
//! which of them can enter a performance rating.

use crate::api::ApiResult;
use crate::player_directory::DISPLAY_NAME;
use sqlx::PgPool;
use uuid::Uuid;

/// Match sides `(match_id, side)` of one player scope. Bind the profile or
/// virtual profile id as `$1`; `rated_only` keeps the sides that count for a
/// performance rating.
pub(crate) fn sides_sql(virtual_profile: bool, rated_only: bool) -> &'static str {
    match (virtual_profile, rated_only) {
        (false, false) => {
            "SELECT mp.match_id,mp.side FROM player_aliases a
             JOIN match_players mp ON mp.alias_id=a.id WHERE a.profile_id=$1"
        }
        (false, true) => {
            "SELECT s.match_id,s.side FROM player_match_sides s WHERE s.profile_id=$1 AND s.rated"
        }
        (true, false) => {
            "SELECT s.match_id,s.side FROM virtual_player_sides s WHERE s.virtual_id=$1"
        }
        (true, true) => {
            "SELECT s.match_id,s.side FROM virtual_player_sides s WHERE s.virtual_id=$1 AND s.rated"
        }
    }
}

/// The source of the sides `mp.side` one player scope played in the match
/// `m`; append the bound profile or virtual profile id.
pub(crate) fn match_sides_from(virtual_profile: bool) -> &'static str {
    if virtual_profile {
        "FROM virtual_player_sides mp WHERE mp.match_id=m.id AND mp.virtual_id="
    } else {
        "FROM match_players mp JOIN player_aliases a ON a.id=mp.alias_id WHERE mp.match_id=m.id AND a.profile_id="
    }
}

/// The lowercase match names that belong to the chosen player. A virtual
/// profile's name selects every alias of its members. Otherwise the name as
/// given plus every alias of its profile; a name that matches several profiles
/// stays on its own, because merging would mix different people.
pub(crate) async fn names(pool: &PgPool, player: &str) -> ApiResult<Vec<String>> {
    let player = player.trim().to_lowercase();
    if let Some(id) = virtual_by_name(pool, &player).await? {
        let mut names: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT lower(btrim(a.name)) FROM virtual_player_members v
             JOIN player_aliases a ON a.profile_id=v.profile_id WHERE v.virtual_id=$1",
        )
        .bind(id)
        .fetch_all(pool)
        .await
        .map_err(anyhow::Error::from)?;
        names.sort();
        return Ok(names);
    }
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

/// A virtual profile's name takes precedence over a real profile's.
async fn virtual_by_name(pool: &PgPool, lowercase: &str) -> ApiResult<Option<Uuid>> {
    Ok(
        sqlx::query_scalar("SELECT id FROM virtual_player_profiles WHERE lower(name)=$1")
            .bind(lowercase)
            .fetch_optional(pool)
            .await
            .map_err(anyhow::Error::from)?,
    )
}

/// Replace each match name by its profile's display name, so the aliases of
/// one profile are offered as a single choice.
pub(crate) async fn display_names(pool: &PgPool, names: Vec<String>) -> ApiResult<Vec<String>> {
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

/// The player choices for match names that occur in the chosen matches: the
/// display names, plus every virtual profile whose members played under one
/// of the names. Members stay choices of their own.
pub(crate) async fn choices(pool: &PgPool, names: Vec<String>) -> ApiResult<Vec<String>> {
    let lowercase: Vec<String> = names
        .iter()
        .map(|name| name.trim().to_lowercase())
        .collect();
    let mut shown = display_names(pool, names).await?;
    let virtual_names: Vec<String> = sqlx::query_scalar(
        "SELECT vp.name FROM virtual_player_profiles vp
         WHERE EXISTS(SELECT 1 FROM virtual_player_members v
                      JOIN player_aliases a ON a.profile_id=v.profile_id
                      WHERE v.virtual_id=vp.id AND lower(btrim(a.name))=ANY($1))",
    )
    .bind(lowercase)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    shown.extend(virtual_names);
    Ok(shown)
}

/// The choice that stands for "me": the virtual profile chosen in Settings,
/// or else the configured player name's profile.
pub(crate) async fn my_choice(
    pool: &PgPool,
    configured: Option<String>,
) -> ApiResult<Option<String>> {
    if let Some(name) = my_virtual_profile(pool).await?.map(|(_, name)| name) {
        return Ok(Some(name));
    }
    let Some(configured) = configured else {
        return Ok(None);
    };
    Ok(display_names(pool, vec![configured])
        .await?
        .into_iter()
        .next())
}

/// The virtual profile chosen as "My player profile", if any.
pub(crate) async fn my_virtual_profile(pool: &PgPool) -> ApiResult<Option<(Uuid, String)>> {
    Ok(
        sqlx::query_as("SELECT id,name FROM virtual_player_profiles WHERE is_me")
            .fetch_optional(pool)
            .await
            .map_err(anyhow::Error::from)?,
    )
}
