use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
    import,
    model::{
        MatchAnalysisSummary, PerformanceRatingSummary, PlayerAnalysisSummary,
        PlayerPerformanceRating,
    },
    store,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

fn app(pool: &PgPool) -> axum::Router {
    api::router(AppState::new(
        pool.clone(),
        Engine::new("/unavailable".into()),
    ))
}

async fn call(app: &axum::Router, method: &str, uri: &str, data: Value, status: u16) -> Value {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(if data.is_null() {
            Body::empty()
        } else {
            Body::from(data.to_string())
        })
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let actual = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        actual,
        status,
        "{method} {uri}: {}",
        String::from_utf8_lossy(&body)
    );
    serde_json::from_slice(&body).unwrap_or(Value::Null)
}

/// Imports a 7-point match with one opening move for White.
async fn import(pool: &PgPool, white: &str, black: &str, site: &str, note: &str) -> Uuid {
    let place = if site.is_empty() {
        String::new()
    } else {
        format!("PC[{site}]")
    };
    let sgf = format!("(;GM[6]PW[{white}]PB[{black}]{place}MI[length:7]C[{note}];W[31hefe])");
    store::import_match(
        pool,
        &format!("{note}.sgf"),
        "sgf",
        sgf.as_bytes(),
        &sgf,
        &import::parse_sgf(&sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id
}

/// A completed match analysis whose rating for `side` is 500 · loss / decisions.
async fn rate(pool: &PgPool, match_id: Uuid, side: usize, loss: f64, decisions: i32) {
    let mut players = [
        PlayerPerformanceRating::default(),
        PlayerPerformanceRating {
            player: 1,
            ..Default::default()
        },
    ];
    players[side].value = Some(500.0 * loss / f64::from(decisions));
    players[side].normalized_equity_loss = loss;
    players[side].unforced_checker_moves = decisions;
    let result = MatchAnalysisSummary {
        engine: "GNUbg".into(),
        plies: 1,
        total_games: 1,
        completed_games: 1,
        total_moves: 1,
        analyzed_moves: 1,
        players: [
            PlayerAnalysisSummary::default(),
            PlayerAnalysisSummary {
                player: 1,
                ..Default::default()
            },
        ],
        performance_rating: Some(PerformanceRatingSummary {
            method: "gnubg_emg_500_v1".into(),
            profile: "GNUbg; 1-ply cubeful deterministic".into(),
            games: 1,
            players,
        }),
    };
    sqlx::query(
        "INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,result)
         VALUES(gen_random_uuid(),$1,'match',1,'completed',1,1,$2)",
    )
    .bind(match_id)
    .bind(serde_json::to_value(result).unwrap())
    .execute(pool)
    .await
    .unwrap();
}

/// Turns an imported match into a finished NeonDojo bot match, as the bot
/// library stores it: no catalogued players, a session with the human side.
async fn bot_match(pool: &PgPool, match_id: Uuid, human: i32, mode: &str, assisted: bool) {
    sqlx::query(
        "UPDATE matches SET play_mode=$2,play_assisted=$3,
           database_id='00000000-0000-0000-0000-000000000003' WHERE id=$1",
    )
    .bind(match_id)
    .bind(mode)
    .bind(assisted)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id,library_archived)
         VALUES($1,$2,'/unavailable','','saved',$3,true)",
    )
    .bind(Uuid::new_v4())
    .bind(json!({"phase":"match_over","play_mode":mode,"human_player":human,"match_length":7}))
    .bind(match_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn profile_id(app: &axum::Router, name: &str, platform: &str) -> (String, i64) {
    let page = call(
        app,
        "GET",
        &format!("/api/players?q={name}"),
        Value::Null,
        200,
    )
    .await;
    for item in page["items"].as_array().unwrap() {
        let details = call(
            app,
            "GET",
            &format!("/api/players/{}", item["id"].as_str().unwrap()),
            Value::Null,
            200,
        )
        .await;
        if details["aliases"]
            .as_array()
            .unwrap()
            .iter()
            .any(|alias| alias["name"] == name && alias["platform"] == platform)
        {
            return (
                item["id"].as_str().unwrap().to_owned(),
                item["version"].as_i64().unwrap(),
            );
        }
    }
    panic!("no profile {name} on {platform}");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn virtual_profiles_group_members_reversibly_and_consolidate_results(pool: PgPool) {
    let galaxy = import(&pool, "leihande", "Carol", "BackgammonGalaxy", "galaxy").await;
    let open = import(&pool, "Dave", "leihande", "OpenGammon", "open").await;
    import(&pool, "Erin", "Frank", "OpenGammon", "other").await;
    rate(&pool, galaxy, 0, 0.04, 10).await;
    rate(&pool, open, 1, 0.12, 30).await;
    let app = app(&pool);
    let (on_galaxy, _) = profile_id(&app, "leihande", "backgammongalaxy.com").await;
    let (on_open, _) = profile_id(&app, "leihande", "opengammon.com").await;

    let created = call(
        &app,
        "POST",
        "/api/virtual-players",
        json!({"name":"  Hendrik Reh "}),
        201,
    )
    .await;
    assert_eq!(created["name"], "Hendrik Reh");
    assert_eq!(created["member_count"], 0);
    assert_eq!(created["match_count"], 0);
    let id = created["id"].as_str().unwrap().to_owned();
    call(
        &app,
        "POST",
        "/api/virtual-players",
        json!({"name":"hendrik reh"}),
        409,
    )
    .await;
    call(
        &app,
        "POST",
        "/api/virtual-players",
        json!({"name":"  "}),
        422,
    )
    .await;

    let uri = format!("/api/virtual-players/{id}");
    let mut version = created["version"].as_i64().unwrap();
    for member in [&on_galaxy, &on_open] {
        let details = call(
            &app,
            "POST",
            &format!("{uri}/members"),
            json!({"profile_id":member,"version":version}),
            200,
        )
        .await;
        version = details["version"].as_i64().unwrap();
    }
    // A stale revision is a conflict.
    call(
        &app,
        "POST",
        &format!("{uri}/members"),
        json!({"profile_id":on_open,"version":0}),
        409,
    )
    .await;

    let details = call(&app, "GET", &uri, Value::Null, 200).await;
    assert_eq!(details["member_count"], 2);
    assert_eq!(details["match_count"], 2);
    assert_eq!(details["game_count"], 2);
    let platforms: Vec<_> = details["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member["aliases"][0]["platform"].clone())
        .collect();
    assert_eq!(platforms.len(), 2);
    assert!(platforms.contains(&json!("backgammongalaxy.com")));
    assert!(platforms.contains(&json!("opengammon.com")));
    // (0.04 + 0.12) · 500 / 40 decisions, from both sides and both sites.
    assert_eq!(
        details["ratings"],
        json!([{"profile":"GNUbg; 1-ply cubeful deterministic","value":2.0,"match_count":2}])
    );
    assert_eq!(details["rating_history"].as_array().unwrap().len(), 2);

    // Members stay in the player list and name their group.
    let listed = call(&app, "GET", "/api/players?q=leihande", Value::Null, 200).await;
    let items = listed["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    for item in items {
        assert_eq!(
            item["virtual_profile"],
            json!({"id":id,"name":"Hendrik Reh"})
        );
        assert_eq!(item["match_count"], 1);
    }
    let member = call(
        &app,
        "GET",
        &format!("/api/players/{on_galaxy}"),
        Value::Null,
        200,
    )
    .await;
    assert_eq!(member["virtual_profile"]["name"], "Hendrik Reh");

    // The library lists the consolidated matches with the group's side.
    let matches = call(
        &app,
        "GET",
        &format!("/api/library/matches?virtual_player_id={id}&page=1"),
        Value::Null,
        200,
    )
    .await;
    assert_eq!(matches["total"], 2);
    let mut sides: Vec<_> = matches["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["white"].as_str().unwrap().to_owned(),
                row["player_sides"].clone(),
            )
        })
        .collect();
    sides.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        sides,
        vec![("Dave".into(), json!([1])), ("leihande".into(), json!([0]))]
    );
    for row in matches["items"].as_array().unwrap() {
        assert!(row["opponent_player_id"].is_string());
    }
    call(
        &app,
        "GET",
        &format!("/api/library/matches?virtual_player_id={id}&player_id={on_open}"),
        Value::Null,
        422,
    )
    .await;

    // A real profile belongs to at most one virtual profile.
    let other = call(
        &app,
        "POST",
        "/api/virtual-players",
        json!({"name":"Someone else"}),
        201,
    )
    .await;
    call(
        &app,
        "POST",
        &format!(
            "/api/virtual-players/{}/members",
            other["id"].as_str().unwrap()
        ),
        json!({"profile_id":on_open,"version":other["version"]}),
        409,
    )
    .await;

    // Removing a member and deleting the group leave the real profiles intact.
    let removed = call(
        &app,
        "DELETE",
        &format!("{uri}/members/{on_open}?version={version}"),
        Value::Null,
        200,
    )
    .await;
    assert_eq!(removed["member_count"], 1);
    assert_eq!(removed["match_count"], 1);
    let free = call(
        &app,
        "GET",
        &format!("/api/players/{on_open}"),
        Value::Null,
        200,
    )
    .await;
    assert!(free.get("virtual_profile").is_none());
    assert_eq!(free["match_count"], 1);
    let renamed = call(
        &app,
        "PUT",
        &uri,
        json!({"name":"Someone else","version":removed["version"]}),
        409,
    )
    .await;
    assert!(
        renamed["error"]
            .as_str()
            .unwrap_or_default()
            .contains("already exists"),
        "{renamed}"
    );
    let renamed = call(
        &app,
        "PUT",
        &uri,
        json!({"name":"H. Reh","version":removed["version"]}),
        200,
    )
    .await;
    call(
        &app,
        "DELETE",
        &format!("{uri}?version={}", removed["version"]),
        Value::Null,
        409,
    )
    .await;
    call(
        &app,
        "DELETE",
        &format!("{uri}?version={}", renamed["version"]),
        Value::Null,
        204,
    )
    .await;
    call(&app, "GET", &uri, Value::Null, 404).await;
    let kept = call(
        &app,
        "GET",
        &format!("/api/players/{on_galaxy}"),
        Value::Null,
        200,
    )
    .await;
    assert!(kept.get("virtual_profile").is_none());
    assert_eq!(kept["match_count"], 1);
    let list = call(&app, "GET", "/api/virtual-players", Value::Null, 200).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn merging_a_member_keeps_its_matches_in_the_group(pool: PgPool) {
    import(&pool, "anna", "Bob", "BackgammonGalaxy", "one").await;
    import(&pool, "anna", "Bob", "OpenGammon", "two").await;
    import(&pool, "anna_b", "Carl", "OpenGammon", "three").await;
    let app = app(&pool);
    let (galaxy, galaxy_version) = profile_id(&app, "anna", "backgammongalaxy.com").await;
    let (open, open_version) = profile_id(&app, "anna", "opengammon.com").await;
    let (second, second_version) = profile_id(&app, "anna_b", "opengammon.com").await;
    let group = call(
        &app,
        "POST",
        "/api/virtual-players",
        json!({"name":"Anna"}),
        201,
    )
    .await;
    let id = group["id"].as_str().unwrap().to_owned();
    call(
        &app,
        "POST",
        &format!("/api/virtual-players/{id}/members"),
        json!({"profile_id":galaxy,"version":group["version"]}),
        200,
    )
    .await;

    // The destination has no group and takes the source's.
    let merged = call(
        &app,
        "POST",
        &format!("/api/players/{open}/merge"),
        json!({"source_id":galaxy,"source_version":galaxy_version,"version":open_version}),
        200,
    )
    .await;
    assert_eq!(merged["virtual_profile"]["id"], id);
    let details = call(
        &app,
        "GET",
        &format!("/api/virtual-players/{id}"),
        Value::Null,
        200,
    )
    .await;
    assert_eq!(details["member_count"], 1);
    assert_eq!(details["match_count"], 2);

    // A destination in another group keeps it; the source's matches go there.
    let other = call(
        &app,
        "POST",
        "/api/virtual-players",
        json!({"name":"Other"}),
        201,
    )
    .await;
    let other_id = other["id"].as_str().unwrap().to_owned();
    call(
        &app,
        "POST",
        &format!("/api/virtual-players/{other_id}/members"),
        json!({"profile_id":second,"version":other["version"]}),
        200,
    )
    .await;
    let merged = call(
        &app,
        "POST",
        &format!("/api/players/{second}/merge"),
        json!({"source_id":open,"source_version":merged["version"],"version":second_version}),
        200,
    )
    .await;
    assert_eq!(merged["virtual_profile"]["id"], other_id);
    let emptied = call(
        &app,
        "GET",
        &format!("/api/virtual-players/{id}"),
        Value::Null,
        200,
    )
    .await;
    assert_eq!(emptied["member_count"], 0);
    assert_eq!(emptied["match_count"], 0);
    let other = call(
        &app,
        "GET",
        &format!("/api/virtual-players/{other_id}"),
        Value::Null,
        200,
    )
    .await;
    assert_eq!(other["match_count"], 3);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn my_virtual_profile_counts_bot_matches_and_is_the_default_study_player(pool: PgPool) {
    let online = import(&pool, "leihande", "Carol", "OpenGammon", "online").await;
    let competition = import(&pool, "GNUbg", "Hendrik", "", "competition").await;
    let coached = import(&pool, "Hendrik", "GNUbg", "", "coached").await;
    let live = import(&pool, "Hendrik", "GNUbg", "", "live").await;
    bot_match(&pool, competition, 1, "competition", false).await;
    bot_match(&pool, coached, 0, "coach", true).await;
    bot_match(&pool, live, 0, "competition", false).await;
    sqlx::query(
        "UPDATE play_sessions SET state=state||'{\"phase\":\"move\"}' WHERE library_match_id=$1",
    )
    .bind(live)
    .execute(&pool)
    .await
    .unwrap();
    rate(&pool, online, 0, 0.04, 10).await;
    rate(&pool, competition, 1, 0.12, 30).await;
    rate(&pool, coached, 0, 0.4, 10).await;
    let app = app(&pool);
    let (member, _) = profile_id(&app, "leihande", "opengammon.com").await;
    let group = call(
        &app,
        "POST",
        "/api/virtual-players",
        json!({"name":"Hendrik Reh"}),
        201,
    )
    .await;
    let id = group["id"].as_str().unwrap().to_owned();
    let uri = format!("/api/virtual-players/{id}");
    call(
        &app,
        "POST",
        &format!("{uri}/members"),
        json!({"profile_id":member,"version":group["version"]}),
        200,
    )
    .await;

    // Without "me", bot matches stay out.
    let before = call(&app, "GET", &uri, Value::Null, 200).await;
    assert_eq!(before["match_count"], 1);
    assert_eq!(before["rateable_match_count"], 1);
    assert_eq!(
        call(&app, "GET", "/api/virtual-players/me", Value::Null, 200).await,
        json!({"profile":null})
    );
    let mistakes = call(
        &app,
        "GET",
        "/api/study/mistakes?resolve=Hendrik",
        Value::Null,
        200,
    )
    .await;
    assert!(
        mistakes["players"]
            .as_array()
            .unwrap()
            .contains(&json!("Hendrik Reh"))
    );
    assert_eq!(mistakes["resolved_player"], "Hendrik");

    call(
        &app,
        "PUT",
        "/api/virtual-players/me",
        json!({"id":Uuid::new_v4()}),
        422,
    )
    .await;
    let me = call(
        &app,
        "PUT",
        "/api/virtual-players/me",
        json!({"id":id}),
        200,
    )
    .await;
    assert_eq!(me, json!({"profile":{"id":id,"name":"Hendrik Reh"}}));
    // Finished bot matches count; the live competition game does not.
    let details = call(&app, "GET", &uri, Value::Null, 200).await;
    assert_eq!(details["is_me"], true);
    assert_eq!(details["match_count"], 3);
    // The coached match can never enter the PR, so it is not waiting for one.
    assert_eq!(details["rateable_match_count"], 2);
    // Only unassisted competition play enters the rating: (0.04 + 0.12) · 500 / 40.
    assert_eq!(details["ratings"][0]["value"], 2.0);
    assert_eq!(details["ratings"][0]["match_count"], 2);
    let matches = call(
        &app,
        "GET",
        &format!("/api/library/matches?virtual_player_id={id}&page=1"),
        Value::Null,
        200,
    )
    .await;
    assert_eq!(matches["total"], 3);
    for row in matches["items"].as_array().unwrap() {
        let human = if row["white"] == "GNUbg" { 1 } else { 0 };
        assert_eq!(row["player_sides"], json!([human]), "{row}");
    }

    // "Me" is the default Study player, whatever the configured name.
    let mistakes = call(
        &app,
        "GET",
        "/api/study/mistakes?resolve=Hendrik",
        Value::Null,
        200,
    )
    .await;
    assert_eq!(mistakes["resolved_player"], "Hendrik Reh");
    let mistakes = call(&app, "GET", "/api/study/mistakes", Value::Null, 200).await;
    assert_eq!(mistakes["resolved_player"], "Hendrik Reh");
    // Its matches are the members' and the bot matches', under any name; like
    // every name-based count, coverage includes the live game.
    let mistakes = call(
        &app,
        "GET",
        "/api/study/mistakes?player=hendrik%20reh",
        Value::Null,
        200,
    )
    .await;
    assert_eq!(mistakes["player_matches"], 4);
    let independent = call(
        &app,
        "GET",
        "/api/study/mistakes?player=Hendrik%20Reh&independent_only=true",
        Value::Null,
        200,
    )
    .await;
    assert_eq!(independent["player_matches"], 3);
    // Members stay choices of their own.
    let alone = call(
        &app,
        "GET",
        "/api/study/mistakes?player=leihande",
        Value::Null,
        200,
    )
    .await;
    assert_eq!(alone["player_matches"], 1);

    let cleared = call(
        &app,
        "PUT",
        "/api/virtual-players/me",
        json!({"id":null}),
        200,
    )
    .await;
    assert_eq!(cleared, json!({"profile":null}));
    let details = call(&app, "GET", &uri, Value::Null, 200).await;
    assert_eq!(details["match_count"], 1);
    assert_eq!(details["rateable_match_count"], 1);
}
