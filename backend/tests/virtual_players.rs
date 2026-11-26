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
/// library stores it: the human side catalogued under the NeonDojo platform,
/// the bot side uncatalogued, and a session with the human side.
async fn bot_match(pool: &PgPool, match_id: Uuid, human: i32, mode: &str, assisted: bool) {
    sqlx::query(
        "UPDATE matches SET play_mode=$2,play_assisted=$3,play_human_side=$4,
           database_id='00000000-0000-0000-0000-000000000003' WHERE id=$1",
    )
    .bind(match_id)
    .bind(mode)
    .bind(assisted)
    .bind(human as i16)
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
async fn bot_matches_are_real_players_that_a_virtual_profile_groups(pool: PgPool) {
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

    // The human side of every bot match is a real player on NeonDojo; the
    // bot side belongs to nobody.
    let (bot_player, _) = profile_id(&app, "Hendrik", "neondojo").await;
    let found = call(&app, "GET", "/api/players?q=hendrik", Value::Null, 200).await;
    assert_eq!(found["items"].as_array().unwrap().len(), 1, "{found}");
    assert_eq!(found["items"][0]["match_count"], 3);
    let nobody = call(&app, "GET", "/api/players?q=gnubg", Value::Null, 200).await;
    assert_eq!(nobody["items"], json!([]), "{nobody}");
    let real = call(
        &app,
        "GET",
        &format!("/api/players/{bot_player}"),
        Value::Null,
        200,
    )
    .await;
    assert_eq!(
        real["aliases"],
        json!([{"platform":"neondojo","name":"Hendrik"}])
    );
    assert_eq!(real["match_count"], 3);
    // Coached and still running competition play cannot enter the PR.
    assert_eq!(real["rateable_match_count"], 1);
    assert_eq!(real["ratings"][0]["value"], 2.0);
    assert_eq!(real["ratings"][0]["match_count"], 1);
    let matches = call(
        &app,
        "GET",
        &format!("/api/library/matches?player_id={bot_player}&page=1"),
        Value::Null,
        200,
    )
    .await;
    assert_eq!(matches["total"], 3);
    for row in matches["items"].as_array().unwrap() {
        let human = if row["white"] == "GNUbg" { 1 } else { 0 };
        assert_eq!(row["player_sides"], json!([human]), "{row}");
        assert_eq!(row["opponent_player_id"], Value::Null, "{row}");
    }

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
    call(
        &app,
        "PUT",
        "/api/virtual-players/me",
        json!({"id":id}),
        200,
    )
    .await;
    // "Me" owns no matches of its own: bot matches count once their player
    // is a member.
    let alone = call(&app, "GET", &uri, Value::Null, 200).await;
    assert_eq!(alone["match_count"], 1);
    assert_eq!(alone["rateable_match_count"], 1);
    super::recurring_mistakes::prepare_decision_index(&pool).await;
    let mistakes = call(
        &app,
        "GET",
        "/api/study/mistakes?player=hendrik%20reh",
        Value::Null,
        200,
    )
    .await;
    assert_eq!(mistakes["player_matches"], 1);

    let me = call(&app, "GET", "/api/virtual-players/me", Value::Null, 200).await;
    assert_eq!(me, json!({"profile":{"id":id,"name":"Hendrik Reh"}}));
    let details = call(
        &app,
        "POST",
        &format!("{uri}/members"),
        json!({"profile_id":bot_player,"version":alone["version"]}),
        200,
    )
    .await;
    assert_eq!(details["is_me"], true);
    assert_eq!(details["match_count"], 4);
    // The coached and the live match are not waiting for a PR.
    assert_eq!(details["rateable_match_count"], 2);
    // Only unassisted, finished competition play enters the rating:
    // (0.04 + 0.12) · 500 / 40.
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
    assert_eq!(matches["total"], 4);
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
    assert!(
        mistakes["players"]
            .as_array()
            .unwrap()
            .contains(&json!("Hendrik Reh"))
    );
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

    // Choosing no "me" changes no group.
    call(
        &app,
        "PUT",
        "/api/virtual-players/me",
        json!({"id":null}),
        200,
    )
    .await;
    let details = call(&app, "GET", &uri, Value::Null, 200).await;
    assert_eq!(details["match_count"], 4);
    assert_eq!(details["rateable_match_count"], 2);
}

#[sqlx::test(migrations = false)]
async fn migration_catalogues_bot_matches_and_keeps_them_in_my_profile(pool: PgPool) {
    let all = sqlx::migrate!("./migrations");
    let before = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter().filter(|m| m.version < 85).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    before.run(&pool).await.unwrap();
    // The current importer writes columns from later migrations, so the
    // pre-0085 matches are inserted with that schema's columns only.
    let legacy = |white: &'static str, black: &'static str, platform: &'static str| {
        let pool = pool.clone();
        async move {
            let id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,player_source_platform)
                 VALUES($1,$2,'sgf',$3,$4,$5,7,1,''::bytea,'',$6)",
            )
            .bind(id)
            .bind(format!("{id}.sgf"))
            .bind(id.to_string())
            .bind(white)
            .bind(black)
            .bind(platform)
            .execute(&pool)
            .await
            .unwrap();
            id
        }
    };
    let online = legacy("leihande", "Carol", "opengammon.com").await;
    let played = legacy("GNUbg", "Hendrik", "").await;
    let orphaned = legacy("Hendrik", "GNUbg", "").await;
    for (id, human) in [(played, Some(1)), (orphaned, None)] {
        sqlx::query(
            "UPDATE matches SET play_mode='competition',
               database_id='00000000-0000-0000-0000-000000000003' WHERE id=$1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        if let Some(human) = human {
            sqlx::query(
                "INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id,library_archived)
                 VALUES($1,$2,'/unavailable','','saved',$3,true)",
            )
            .bind(Uuid::new_v4())
            .bind(json!({"phase":"match_over","play_mode":"competition","human_player":human,"match_length":7}))
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    let member: Uuid = sqlx::query_scalar(
        "SELECT profile_id FROM player_aliases WHERE platform='opengammon.com' AND name='leihande'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let me: Uuid = sqlx::query_scalar(
        "INSERT INTO virtual_player_profiles(name,is_me) VALUES('Hendrik Reh',true) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO virtual_player_members(profile_id,virtual_id) VALUES($1,$2)")
        .bind(member)
        .bind(me)
        .execute(&pool)
        .await
        .unwrap();
    let sides = |pool: PgPool| async move {
        let rows: Vec<(Uuid, i16)> =
            sqlx::query_as("SELECT match_id,side FROM virtual_player_sides ORDER BY match_id,side")
                .fetch_all(&pool)
                .await
                .unwrap();
        rows
    };
    let mut expected = vec![(online, 0), (played, 1)];
    expected.sort();
    assert_eq!(sides(pool.clone()).await, expected);

    all.run(&pool).await.unwrap();

    // The played match now belongs to a real NeonDojo player; the one without
    // a session has no known human side and stays uncatalogued.
    let catalogued: Vec<(Uuid, i16, String, String)> = sqlx::query_as(
        "SELECT mp.match_id,mp.side,a.platform,a.name FROM match_players mp
         JOIN player_aliases a ON a.id=mp.alias_id
         WHERE mp.match_id=ANY($1) ORDER BY mp.match_id",
    )
    .bind(vec![played, orphaned])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        catalogued,
        [(played, 1, "neondojo".to_owned(), "Hendrik".to_owned())]
    );
    // That player joins "me", so its results stay the same.
    assert_eq!(sides(pool.clone()).await, expected);
    let version: i64 =
        sqlx::query_scalar("SELECT version FROM virtual_player_profiles WHERE id=$1")
            .bind(me)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(version, 1);
}

async fn virtual_profile(app: &axum::Router, name: &str) -> Uuid {
    let created = call(
        app,
        "POST",
        "/api/virtual-players",
        json!({"name":name}),
        201,
    )
    .await;
    created["id"].as_str().unwrap().parse().unwrap()
}

fn choose_me(app: &axum::Router, id: Uuid) -> tokio::task::JoinHandle<(u16, Value)> {
    let app = app.clone();
    tokio::spawn(async move {
        crate::support::send(&app, "PUT", "/api/virtual-players/me", json!({"id":id})).await
    })
}

async fn lock_virtual_profiles(tx: &mut sqlx::PgConnection, ids: &[Uuid]) {
    sqlx::query("SELECT id FROM virtual_player_profiles WHERE id=ANY($1) FOR UPDATE")
        .bind(ids)
        .execute(tx)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn virtual_me_concurrent_choices_leave_exactly_one_profile(pool: PgPool) {
    let app = app(&pool);
    let first = virtual_profile(&app, "First").await;
    let second = virtual_profile(&app, "Second").await;
    // Hold both choices at their profile rows, so that both pass their
    // checks before either writes unless they serialize.
    let mut gate = pool.begin().await.unwrap();
    lock_virtual_profiles(&mut gate, &[first, second]).await;
    let choices = [choose_me(&app, first), choose_me(&app, second)];
    crate::support::wait_for_lock_waiters(&pool, "%", 2).await;
    gate.commit().await.unwrap();
    for choice in choices {
        let (status, body) = choice.await.unwrap();
        assert_eq!(status, 200, "{body}");
    }
    let chosen: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM virtual_player_profiles WHERE is_me")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(chosen.len(), 1);
    assert!([first, second].contains(&chosen[0]));
    let me = call(&app, "GET", "/api/virtual-players/me", Value::Null, 200).await;
    assert_eq!(me["profile"]["id"], json!(chosen[0]));
}

#[sqlx::test(migrations = "./migrations")]
async fn virtual_me_choice_does_not_deadlock_with_a_rename_in_progress(pool: PgPool) {
    let app = app(&pool);
    let id = virtual_profile(&app, "Before").await;
    // A rename between its revision lock and its update.
    let mut rename = pool.begin().await.unwrap();
    lock_virtual_profiles(&mut rename, &[id]).await;
    let choice = choose_me(&app, id);
    crate::support::wait_for_lock_waiters(&pool, "%", 1).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        sqlx::query(
            "UPDATE virtual_player_profiles SET name='After',version=version+1 WHERE id=$1",
        )
        .bind(id)
        .execute(&mut *rename),
    )
    .await
    .expect("the rename must not wait for the pending choice")
    .expect("the rename must not deadlock with the pending choice");
    rename.commit().await.unwrap();
    let (status, body) = choice.await.unwrap();
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["profile"], json!({"id":id,"name":"After"}));
}

#[sqlx::test(migrations = "./migrations")]
async fn virtual_profile_create_does_not_wait_for_a_pending_me_choice(pool: PgPool) {
    let app = app(&pool);
    let id = virtual_profile(&app, "Chosen").await;
    let mut gate = pool.begin().await.unwrap();
    lock_virtual_profiles(&mut gate, &[id]).await;
    let choice = choose_me(&app, id);
    crate::support::wait_for_lock_waiters(&pool, "%", 1).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        virtual_profile(&app, "Unrelated"),
    )
    .await
    .expect("creating a profile must not wait for a pending choice of me");
    gate.commit().await.unwrap();
    let (status, body) = choice.await.unwrap();
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["profile"]["id"], json!(id));
}
