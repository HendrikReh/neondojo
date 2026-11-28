//! Match context for a cube decision: away scores, cube state and the
//! dead-cube take point from a match equity table.

use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use crate::backgammon::ids::{Field, MatchId};
use crate::json_errors::Json;
pub use crate::practice::evaluation::CubeChances;
use axum::{Router, extract::State, routing::post};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use utoipa::{OpenApi, ToSchema};

const MET_JSON: &str = include_str!("met_kazaross_xg2.json");
const MET_LENGTH: usize = 25;

#[derive(Deserialize)]
struct MatchEquityTable {
    name: String,
    /// Row: the player's away score minus one. Column: the opponent's.
    /// A one-away entry is the Crawford game.
    pre_crawford: Vec<Vec<f64>>,
    /// The trailer's match winning chance after the Crawford game, by away score.
    post_crawford: Vec<f64>,
}

fn table() -> &'static MatchEquityTable {
    static TABLE: OnceLock<MatchEquityTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let table: MatchEquityTable =
            serde_json::from_str(MET_JSON).expect("embedded match equity table is valid JSON");
        assert!(
            table.pre_crawford.len() == MET_LENGTH
                && table.pre_crawford.iter().all(|row| row.len() == MET_LENGTH)
                && table.post_crawford.len() == MET_LENGTH,
            "embedded match equity table has the wrong shape"
        );
        table
    })
}

/// Match winning chance for a player who needs `away` points against an
/// opponent who needs `opponent_away`, at the start of a game.
/// `post_crawford` selects the table used when exactly one side is one away.
fn match_winning_chance(away: i32, opponent_away: i32, post_crawford: bool) -> Option<f64> {
    if away <= 0 {
        return Some(1.0);
    }
    if opponent_away <= 0 {
        return Some(0.0);
    }
    let index = |value: i32| {
        usize::try_from(value - 1)
            .ok()
            .filter(|index| *index < MET_LENGTH)
    };
    let (own, other) = (index(away)?, index(opponent_away)?);
    let table = table();
    if post_crawford && away != opponent_away {
        if opponent_away == 1 {
            return Some(table.post_crawford[own]);
        }
        if away == 1 {
            return Some(1.0 - table.post_crawford[other]);
        }
    }
    Some(table.pre_crawford[own][other])
}

/// Minimum game winning chance at which taking breaks even, ignoring gammons
/// and any later use of the cube. `away` is `[taker, doubler]`; `cube` is the
/// value before the offer.
fn match_take_point(away: [i32; 2], cube: i32, post_crawford: bool) -> Option<f64> {
    let [taker, doubler] = away;
    // The score after this game decides which table applies: once the
    // Crawford game has been played, a one-away score stays post-Crawford.
    let chance = |taker_away: i32, doubler_away: i32| {
        match_winning_chance(taker_away, doubler_away, post_crawford)
    };
    let pass = chance(taker, doubler - cube)?;
    let win = chance(taker - 2 * cube, doubler)?;
    let lose = chance(taker, doubler - 2 * cube)?;
    let range = win - lose;
    (range > 0.0).then(|| ((pass - lose) / range).clamp(0.0, 1.0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TakePointBasis {
    /// 25 % in money play with a dead cube and no gammons.
    Money,
    /// Derived from the match equity table at this score.
    MatchEquityTable,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TakePoint {
    /// Game winning chance the taker needs, from 0 to 1.
    pub value: f64,
    pub basis: TakePointBasis,
    /// Match equity table name; absent in money play.
    pub table: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CubeContext {
    /// 0 is money play.
    pub match_length: i32,
    /// Points scored so far by White and Black.
    pub scores: [i32; 2],
    /// Points still needed by White and Black; absent in money play.
    pub away: Option<[i32; 2]>,
    pub crawford: bool,
    /// A one-away score after the Crawford game has been played.
    pub post_crawford: bool,
    /// Money play only.
    pub jacoby: Option<bool>,
    /// Cube value before a pending offer.
    pub cube_value: i32,
    /// 0 White, 1 Black, absent while the cube is centered.
    pub cube_owner: Option<i32>,
    /// Absolute color of the player who offers or considers the double.
    pub doubler: i32,
    /// Absent when no double is possible or the score is outside the table.
    pub take_point: Option<TakePoint>,
}

pub(crate) fn decision_question_kind(kind: &str, played: &str) -> &'static str {
    if kind == "checker" {
        "checker"
    } else if matches!(played, "take" | "drop") {
        "cube_response"
    } else {
        "cube_offer"
    }
}

pub(crate) fn is_cube_practice_eligible(position: &crate::model::Position, response: bool) -> bool {
    crate::practice::cube_context(position, response).is_ok()
        && (response
            || crate::match_analysis::cube_analysis_available(
                &position.board,
                &position.gnubg_match_id,
                position.player,
                position.cube_enabled != Some(false),
            )
            .unwrap_or(false))
}

pub fn cube_context(match_id: &str, doubler: i32) -> Result<CubeContext, String> {
    if !(0..=1).contains(&doubler) {
        return Err("doubler must be 0 (White) or 1 (Black)".into());
    }
    let id = MatchId::decode(match_id).ok_or("Invalid GNUbg match ID")?;
    let match_length = id.get(Field::LENGTH);
    let scores = [id.get(Field::SCORE_0), id.get(Field::SCORE_1)];
    let cube_log = id.get(Field::CUBE_LOG2);
    let cube_value = 1 << cube_log;
    let cube_owner = match id.get(Field::CUBE_OWNER) {
        3 => None,
        owner @ 0..=1 => Some(owner),
        _ => return Err("Invalid cube owner in match ID".into()),
    };
    let crawford = id.get(Field::CRAWFORD) == 1;
    if match_length > 0 && scores.iter().any(|score| *score >= match_length) {
        return Err("Match ID describes a finished match".into());
    }
    let away = (match_length > 0).then(|| scores.map(|score| match_length - score));
    let post_crawford = !crawford && away.is_some_and(|away| away.contains(&1));
    let take_point = if !id.can_double(doubler) {
        None
    } else if let Some(away) = away {
        let taker = usize::try_from(1 - doubler).map_err(|_| "Invalid doubler")?;
        let doubler = usize::try_from(doubler).map_err(|_| "Invalid doubler")?;
        match_take_point([away[taker], away[doubler]], cube_value, post_crawford).map(|value| {
            TakePoint {
                value,
                basis: TakePointBasis::MatchEquityTable,
                table: Some(table().name.clone()),
            }
        })
    } else {
        Some(TakePoint {
            value: 0.25,
            basis: TakePointBasis::Money,
            table: None,
        })
    };
    Ok(CubeContext {
        match_length,
        scores,
        away,
        crawford,
        post_crawford,
        jacoby: (match_length == 0).then(|| id.get(Field::NO_JACOBY) == 0),
        cube_value,
        cube_owner,
        doubler,
        take_point,
    })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CubeContextRequest {
    /// GNUbg match ID of the decision; a pending offer keeps the previous cube value.
    #[schema(min_length = 12, max_length = 12)]
    pub match_id: String,
    /// Absolute color of the player who doubles: 0 White, 1 Black.
    #[schema(minimum = 0, maximum = 1)]
    pub doubler: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CubeChancesRequest {
    /// The position of the cube decision, with its match context and the
    /// deciding player on roll.
    pub position: crate::model::Position,
    /// True for take or pass, where the opponent doubled.
    pub response: bool,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/cube-context", post(context))
        .route("/api/cube-chances", post(chances))
}

/// Calculate the cubeless chances of a cube decision whose saved analysis
/// has none. The result names its own engine and depth and is not stored.
#[utoipa::path(post, path = "/api/cube-chances", request_body = CubeChancesRequest,
    responses((status = 200, body = CubeChances), (status = 422, body = ErrorBody),
              (status = 429, body = ErrorBody), (status = 503, body = ErrorBody),
              (status = 504, body = ErrorBody)),
    tag = "Match analysis")]
async fn chances(
    State(state): State<AppState>,
    request: Result<Json<CubeChancesRequest>, ApiError>,
) -> ApiResult<Json<CubeChances>> {
    let Json(request) = request.map_err(|_| ApiError::invalid("Invalid cube chances request"))?;
    crate::practice::evaluation::cube_chances(&state, &request.position, request.response)
        .await
        .map(Json)
}

/// Describe the score, cube and take point of a cube decision.
#[utoipa::path(post, path = "/api/cube-context", request_body = CubeContextRequest,
    responses((status = 200, body = CubeContext), (status = 422, body = ErrorBody)),
    tag = "Match analysis")]
async fn context(
    request: Result<Json<CubeContextRequest>, ApiError>,
) -> ApiResult<Json<CubeContext>> {
    let Json(request) = request.map_err(|_| ApiError::invalid("Invalid cube context request"))?;
    cube_context(&request.match_id, request.doubler)
        .map(Json)
        .map_err(|message| ApiError::invalid(&message))
}

#[derive(OpenApi)]
#[openapi(
    paths(context, chances),
    components(schemas(
        CubeContextRequest,
        CubeContext,
        TakePoint,
        TakePointBasis,
        CubeChancesRequest,
        CubeChances
    ))
)]
pub struct CubeContextDoc;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backgammon::ids::match_id;

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.0005,
            "{actual} is not {expected}"
        );
    }

    #[test]
    fn table_is_symmetric_and_matches_published_entries() {
        close(match_winning_chance(2, 4, false).unwrap(), 0.6687);
        close(match_winning_chance(1, 2, false).unwrap(), 0.67736);
        for own in 1..=25 {
            for other in 1..=25 {
                close(
                    match_winning_chance(own, other, false).unwrap()
                        + match_winning_chance(other, own, false).unwrap(),
                    1.0,
                );
            }
        }
        assert_eq!(match_winning_chance(0, 3, false), Some(1.0));
        assert_eq!(match_winning_chance(3, -1, false), Some(0.0));
        assert_eq!(match_winning_chance(26, 3, false), None);
        close(match_winning_chance(3, 1, true).unwrap(), 0.32264);
        close(match_winning_chance(1, 3, true).unwrap(), 1.0 - 0.32264);
    }

    #[test]
    fn take_points_follow_the_table_at_reference_scores() {
        // Doubler 2-away, taker 4-away, centered cube: the double ends the match
        // for the doubler. Pass: 4a/1a Crawford = 0.18564. Win: 2a/2a = 0.5. Lose: 0.
        close(match_take_point([4, 2], 1, false).unwrap(), 0.18564 / 0.5);
        // Both 2-away: pass leaves 2a/1a Crawford = 0.32264; a take plays for the match.
        close(match_take_point([2, 2], 1, false).unwrap(), 0.32264);
        // Long matches approach the money take point of 25 %.
        let long = match_take_point([25, 25], 1, false).unwrap();
        assert!((0.22..0.27).contains(&long), "{long}");
    }

    #[test]
    fn context_decodes_score_cube_and_crawford() {
        // 7-point match, White 0, Black 1, White owns a 2-cube and is on roll.
        let id = match_id(7, [0, 1], 2, 0, false, false, 0, [0, 0]);
        let context = cube_context(&id, 0).unwrap();
        assert_eq!(context.match_length, 7);
        assert_eq!(context.away, Some([7, 6]));
        assert_eq!((context.cube_value, context.cube_owner), (2, Some(0)));
        assert!(!context.crawford && !context.post_crawford);
        let take = context.take_point.unwrap();
        assert_eq!(take.basis, TakePointBasis::MatchEquityTable);
        assert_eq!(take.table.as_deref(), Some("Kazaross-XG2"));
        assert!((0.15..0.35).contains(&take.value));
        // Black cannot redouble a cube White owns.
        assert!(cube_context(&id, 1).unwrap().take_point.is_none());

        let crawford = match_id(5, [4, 2], 1, 3, true, false, 1, [0, 0]);
        let context = cube_context(&crawford, 1).unwrap();
        assert!(context.crawford && !context.post_crawford);
        assert!(context.take_point.is_none());

        let post = match_id(5, [4, 2], 1, 3, false, false, 1, [0, 0]);
        let context = cube_context(&post, 1).unwrap();
        assert!(context.post_crawford);
        // Trailer 3-away doubles: pass leaves 2-away post-Crawford (0.48803 for
        // the trailer); a take decides 2 points: trailer 1-away or match over.
        let value = context.take_point.unwrap().value;
        close(value, (1.0 - 0.48803 - 0.5) / (1.0 - 0.5));

        let money = match_id(0, [0, 0], 1, 3, false, true, 0, [0, 0]);
        let context = cube_context(&money, 0).unwrap();
        assert_eq!(context.away, None);
        assert_eq!(context.jacoby, Some(true));
        assert_eq!(context.take_point.unwrap().basis, TakePointBasis::Money);
        assert!(cube_context("not-an-id", 0).is_err());
        assert!(cube_context(&money, 2).is_err());
    }
}
