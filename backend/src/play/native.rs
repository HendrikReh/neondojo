use super::*;
use crate::async_temp_dir::AsyncTempDir;

/// One request to the GNUbg adapter. Depths are NeonDojo plies here and are
/// written as GNUbg-native levels in `request.json`.
#[derive(Clone, Copy, Serialize)]
pub(super) struct NativeRequest<'a> {
    pub(super) new: bool,
    pub(super) external_bot: bool,
    pub(super) accept_resignation: Option<bool>,
    pub(super) human_player: i32,
    pub(super) match_length: i32,
    #[serde(serialize_with = "native_plies")]
    pub(super) plies: i32,
    pub(super) native_id: &'a str,
    pub(super) action: &'a str,
    pub(super) notation: Option<&'a str>,
    pub(super) resign_level: Option<i32>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "native_bot_plies"
    )]
    pub(super) match_bots: Option<[Option<i32>; 2]>,
    /// Cubeless money duel without the Jacoby rule; requires `match_bots`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) cubeless: bool,
}

pub(super) fn native_plies<S: serde::Serializer>(
    plies: &i32,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let native = crate::engine::gnubg_plies(*plies).map_err(serde::ser::Error::custom)?;
    serializer.serialize_i32(native)
}

pub(super) fn native_bot_plies<S: serde::Serializer>(
    bots: &Option<[Option<i32>; 2]>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let native = bots
        .map(|bots| bots.map(|depth| depth.map(crate::engine::gnubg_plies).transpose()))
        .map(|[a, b]| Ok::<_, anyhow::Error>([a?, b?]))
        .transpose()
        .map_err(serde::ser::Error::custom)?;
    native.serialize(serializer)
}

#[derive(Deserialize)]
pub(super) struct NativeSnapshot {
    pub(super) native_id: String,
    pub(super) board: Vec<i32>,
    pub(super) player: i32,
    pub(super) dice: Vec<i32>,
    pub(super) phase: PlayPhase,
    pub(super) can_double: bool,
    pub(super) legal_moves: Vec<PlayMove>,
    pub(super) history: Vec<PlayHistory>,
    pub(super) scores: [i32; 2],
    pub(super) game_no: i32,
    pub(super) winner: Option<i32>,
    #[serde(default)]
    pub(super) opening: Option<OpeningRoll>,
    #[serde(default)]
    pub(super) resignation_declined: bool,
    #[serde(default)]
    pub(super) match_rolls: Vec<[u8; 2]>,
    /// GNUbg engine work per native player during this step's bot turns.
    #[serde(default)]
    pub(super) bot_costs: Option<[crate::engines::EngineCost; 2]>,
}

pub(crate) async fn probe_native(engine: &Engine) -> anyhow::Result<()> {
    let directory = AsyncTempDir::new().await?;
    tokio::fs::write(directory.path().join("probe.py"),"import gnubg\nassert all(callable(getattr(gnubg,key,None)) for key in ['command','nextturn','board','posinfo','gnubgid','match','hint','navigate'])\nopen('ready.txt','w').write('ready')\n").await?;
    engine
        .run_with_timeout(
            directory.path(),
            "load python probe.py\n",
            std::time::Duration::from_secs(20),
        )
        .await?;
    ensure!(
        tokio::fs::read_to_string(directory.path().join("ready.txt")).await? == "ready",
        "GNUbg embedded Python is unavailable"
    );
    Ok(())
}

pub(super) async fn native(
    session: &PlaySession,
    path: &str,
    previous: Option<(&str, &str)>,
    action: &str,
    notation: Option<&str>,
    resign_level: Option<i32>,
) -> anyhow::Result<(NativeSnapshot, String)> {
    if let Some(snapshot) = session
        .engine_snapshots
        .as_ref()
        .and_then(|snapshots| snapshots.bot.open_sage.as_ref())
    {
        let bot = crate::engines::SageEngine::for_play(snapshot.clone())?;
        return open_sage::advance(
            session,
            &bot,
            &session.rules_engine(path)?,
            previous,
            action,
            notation,
            resign_level,
        )
        .await;
    }
    let request = NativeRequest {
        external_bot: false,
        accept_resignation: None,
        new: previous.is_none(),
        human_player: session.human_player,
        match_length: session.match_length,
        plies: session.engine.plies,
        native_id: previous.map_or("", |(_, id)| id),
        action,
        notation,
        resign_level,
        match_bots: None,
        cubeless: false,
    };
    let engine = session.rules_engine(path)?;
    run_native(&engine, &request, previous.map(|(sgf, _)| sgf)).await
}

pub(crate) async fn benchmark_checker_turn(
    engine: &Engine,
    previous: (&str, &str),
    bot_player: i32,
    dice: [i32; 2],
    plies: i32,
) -> anyhow::Result<()> {
    let request = NativeRequest {
        external_bot: false,
        accept_resignation: None,
        new: false,
        human_player: 1 - bot_player,
        match_length: 11,
        plies,
        native_id: previous.1,
        // Resume with dice already on the board, exactly as for a bot opening.
        action: "start_opening_move",
        notation: None,
        resign_level: None,
        match_bots: None,
        cubeless: false,
    };
    let (snapshot, _) = run_native(engine, &request, Some(previous.0)).await?;
    ensure!(
        matches!(snapshot.history.as_slice(), [turn] if turn.player == bot_player
            && turn.dice == dice && turn.notation != "pass" && !turn.notation.is_empty()),
        "GNUbg did not complete the expected checker turn"
    );
    Ok(())
}

pub(super) async fn run_native(
    engine: &Engine,
    request: &NativeRequest<'_>,
    sgf: Option<&str>,
) -> anyhow::Result<(NativeSnapshot, String)> {
    let directory = AsyncTempDir::new().await?;
    if let Some(sgf) = sgf {
        tokio::fs::write(directory.path().join("input.sgf"), sgf).await?;
    }
    tokio::fs::write(
        directory.path().join("request.json"),
        serde_json::to_vec(request)?,
    )
    .await?;
    tokio::fs::write(
        directory.path().join("adapter.py"),
        include_str!("native.py"),
    )
    .await?;
    let output = engine
        .run_with_timeout(
            directory.path(),
            "load python adapter.py\n",
            std::time::Duration::from_secs(180),
        )
        .await?;
    read_native_result(directory.path(), request, &output).await
}

pub(super) async fn read_native_artifact(
    directory: &std::path::Path,
    name: &str,
) -> anyhow::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    const LIMIT: u64 = 32 * 1024 * 1024;
    let file = tokio::fs::File::open(directory.join(name))
        .await
        .with_context(|| format!("GNUbg produced no complete {name}"))?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes).await?;
    ensure!(bytes.len() <= LIMIT as usize, "GNUbg {name} exceeds limit");
    Ok(bytes)
}

pub(super) async fn read_native_result(
    directory: &std::path::Path,
    request: &NativeRequest<'_>,
    output: &str,
) -> anyhow::Result<(NativeSnapshot, String)> {
    let bytes = read_native_artifact(directory, "snapshot.json").await?;
    let mut snapshot: NativeSnapshot = serde_json::from_slice(&bytes)?;
    if request.action == "opening_roll" {
        let expression = regex::Regex::new(r"(?m)^.* rolls ([1-6]), .* rolls ([1-6])\.\r?$")?;
        let rolls: Vec<[i32; 2]> = expression
            .captures_iter(output)
            .map(|hit| [hit[1].parse().unwrap(), hit[2].parse().unwrap()])
            .collect();
        let last = *rolls
            .last()
            .context("GNUbg did not report its opening dice")?;
        ensure!(
            last[0] != last[1]
                && rolls[..rolls.len() - 1]
                    .iter()
                    .all(|pair| pair[0] == pair[1]),
            "Invalid native opening roll sequence"
        );
        let winner = i32::from(last[1] > last[0]);
        ensure!(
            snapshot.player == winner
                && snapshot.dice == [last[0].max(last[1]), last[0].min(last[1])],
            "Native opening roll result disagrees with checkpoint"
        );
        snapshot.opening = Some(OpeningRoll {
            white_die: last[0],
            black_die: last[1],
            winner,
            rolls,
        });
    }
    let sgf = String::from_utf8(read_native_artifact(directory, "output.sgf").await?)?;
    ensure!(
        !sgf.is_empty() && sgf.len() <= 32 * 1024 * 1024,
        "GNUbg SGF missing or exceeds limit"
    );
    Ok((snapshot, sgf))
}

pub(super) fn apply_snapshot(
    session: &mut PlaySession,
    snapshot: NativeSnapshot,
) -> anyhow::Result<String> {
    ensure!(
        snapshot.board.len() == 26,
        "Native board must have 26 points"
    );
    let (position_id, match_id) = snapshot
        .native_id
        .split_once(':')
        .context("GNUbg ID missing")?;
    session.position = Position {
        cube_enabled: None,
        id: session.id,
        match_uuid: session.id,
        ordinal: session.version.try_into()?,
        game_no: snapshot.game_no,
        ply: snapshot.history.len().try_into()?,
        player: snapshot.player,
        dice: snapshot.dice,
        board: snapshot.board,
        position_id: position_id.into(),
        gnubg_match_id: match_id.into(),
        notation: String::new(),
    };
    session.phase = snapshot.phase;
    session.can_double = snapshot.can_double;
    session.legal_moves = snapshot.legal_moves;
    session.history = snapshot.history;
    session.scores = snapshot.scores;
    session.game_no = snapshot.game_no;
    session.winner = snapshot.winner;
    if snapshot.opening.is_some() {
        session.opening = snapshot.opening;
    }
    Ok(snapshot.native_id)
}

pub(crate) async fn benchmark_sage_checker_turn(
    bot: &crate::engines::SageEngine,
    rules: &Engine,
    previous: (&str, &str),
    bot_player: i32,
    dice: [i32; 2],
) -> anyhow::Result<()> {
    open_sage::benchmark_checker_turn(bot, rules, previous, bot_player, dice).await
}
