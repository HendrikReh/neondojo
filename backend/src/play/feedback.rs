//! Persisted human decision checkpoints and presentation policies.
use super::*;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type,
)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum PlayMode {
    #[default]
    Competition,
    Training,
    Coach,
    Tutor,
}
pub(crate) fn default_play_mode() -> PlayMode {
    PlayMode::Coach
}
pub(crate) fn default_threshold() -> f64 {
    0.04
}
pub(crate) fn default_auto_analysis() -> bool {
    true
}
/// Sessions saved before the depth was recorded used GNUbg's native 2-ply.
pub(crate) fn default_analysis_plies() -> i32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayAlternative {
    #[serde(default)]
    pub rating_decisions: i32,
    pub action: String,
    pub move_index: Option<usize>,
    pub notation: String,
    pub loss: f64,
    pub equity: Option<f64>,
    pub probabilities: Option<Vec<f64>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayAttempt {
    #[serde(default)]
    pub rating_decisions: Option<i32>,
    #[serde(default)]
    pub notation: String,
    pub action: String,
    pub move_index: Option<usize>,
    pub loss: Option<f64>,
    pub severity: Option<String>,
    pub assisted: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayDecision {
    #[serde(default)]
    pub first_rating_decisions: Option<i32>,
    #[serde(default)]
    pub rating_decisions: Option<i32>,
    #[serde(default)]
    pub game_no: i32,
    #[serde(default)]
    pub history_index: usize,
    pub id: Uuid,
    pub action: Option<String>,
    pub move_index: Option<usize>,
    pub original_action: Option<String>,
    pub original_move_index: Option<usize>,
    #[serde(default)]
    pub original_notation: Option<String>,
    #[serde(default)]
    pub notation: Option<String>,
    pub warning: bool,
    pub revealed: bool,
    pub assisted: bool,
    pub retry_count: u32,
    pub first_loss: Option<f64>,
    pub loss: Option<f64>,
    pub first_severity: Option<String>,
    pub severity: Option<String>,
    pub alternatives: Vec<PlayAlternative>,
    pub attempts: Vec<PlayAttempt>,
    pub committed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayReview {
    pub analysis_plies: i32,
    /// Actual played match only; native statistics preserve GNUbg's denominator.
    pub games: Vec<crate::match_analysis::AnalyzedGame>,
    pub assisted: bool,
    /// Sum of first attempted losses; not a performance rating.
    pub first_attempt_loss: f64,
    pub final_choice_loss: f64,
    pub first_attempt_count: usize,
    pub retry_count: u32,
    /// EMG500 of original choices on reached positions, using native relevance.
    pub first_attempt_rating: Option<f64>,
    pub first_attempt_rating_decisions: i32,
    pub first_attempt_rating_method: String,
    pub first_attempt_rating_profile: String,
    pub independent_first_attempt_count: usize,
    pub assisted_first_attempt_count: usize,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayReviewRequest {
    pub version: i64,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackOperation {
    Hint,
    Reveal,
    Retry,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayFeedbackRequest {
    pub version: i64,
    pub operation: FeedbackOperation,
    pub decision_id: Option<Uuid>,
}
pub(super) fn empty_decision(alternatives: Vec<PlayAlternative>) -> PlayDecision {
    PlayDecision {
        first_rating_decisions: None,
        rating_decisions: None,
        game_no: 0,
        history_index: 0,
        id: Uuid::new_v4(),
        action: None,
        move_index: None,
        original_action: None,
        original_move_index: None,
        original_notation: None,
        notation: None,
        warning: false,
        revealed: false,
        assisted: false,
        retry_count: 0,
        first_loss: None,
        loss: None,
        first_severity: None,
        severity: None,
        alternatives,
        attempts: vec![],
        committed: false,
    }
}
pub(super) fn stage_choice(
    session: &mut PlaySession,
    action: &str,
    move_index: Option<usize>,
    alternatives: Vec<PlayAlternative>,
) -> ApiResult<()> {
    let decision = session
        .pending_decision
        .get_or_insert_with(|| empty_decision(alternatives));
    decision.game_no = session.game_no;
    decision.history_index = session.history.len();
    let chosen = decision
        .alternatives
        .iter()
        .find(|a| a.action == action && a.move_index == move_index)
        .ok_or_else(|| ApiError::invalid("Analysis did not cover the selected decision"))?;
    let loss = chosen.loss;
    let mark = session.error_thresholds.classify(loss).as_str();
    let rating_decisions = chosen.rating_decisions;
    let assisted = decision.assisted;
    if decision.original_action.is_none() {
        decision.original_action = Some(action.into());
        decision.original_move_index = move_index;
        decision.original_notation = Some(chosen.notation.clone());
        decision.first_rating_decisions = Some(rating_decisions);
        decision.first_loss = Some(loss);
        decision.first_severity = Some(mark.into());
    }
    decision.attempts.push(PlayAttempt {
        rating_decisions: Some(rating_decisions),
        notation: chosen.notation.clone(),
        action: action.into(),
        move_index,
        loss: Some(loss),
        severity: Some(mark.into()),
        assisted,
    });
    decision.rating_decisions = Some(rating_decisions);
    decision.action = Some(action.into());
    decision.move_index = move_index;
    decision.notation = Some(chosen.notation.clone());
    decision.loss = Some(loss);
    decision.severity = Some(mark.into());
    decision.warning = session.play_mode == PlayMode::Tutor && loss >= session.tutor_threshold;
    if session.play_mode == PlayMode::Coach
        || (session.play_mode == PlayMode::Training && session.training_auto_analysis)
    {
        decision.revealed = true;
    }
    Ok(())
}
pub(super) fn present(mut session: PlaySession) -> PlaySession {
    let complete = super::review_gate::finished(&session);
    for decision in session
        .pending_decision
        .iter_mut()
        .chain(session.decisions.iter_mut())
    {
        if !complete && (session.play_mode == PlayMode::Competition || !decision.revealed) {
            decision.loss = None;
            decision.first_loss = None;
            decision.first_rating_decisions = None;
            decision.rating_decisions = None;
            decision.severity = None;
            decision.first_severity = None;
            decision.alternatives.clear();
            for attempt in &mut decision.attempts {
                attempt.loss = None;
                attempt.severity = None;
                attempt.rating_decisions = None;
            }
        } else if !complete && session.play_mode == PlayMode::Coach {
            for alternative in &mut decision.alternatives {
                alternative.equity = None;
                alternative.probabilities = None;
            }
        }
    }
    session
}

pub(super) fn should_pause_for_feedback(session: &PlaySession, action: &PlayAction) -> bool {
    match session.play_mode {
        PlayMode::Competition => false,
        PlayMode::Training => session.training_auto_analysis && !matches!(action, PlayAction::Roll),
        PlayMode::Coach => match action {
            PlayAction::Pass => false,
            PlayAction::Move | PlayAction::Roll => session
                .pending_decision
                .as_ref()
                .and_then(|decision| decision.loss)
                .is_some_and(|loss| loss >= session.error_thresholds.error),
            _ => true,
        },
        PlayMode::Tutor => session.pending_decision.as_ref().is_some_and(|d| d.warning),
    }
}

/// Preserve every reached competition position in mixed-mode matches. Native
/// actual-play statistics are the baseline; stored attempts replace only the
/// decisions for which feedback captured both original and final choices.
pub(super) fn build_review(
    session: &PlaySession,
    games: Vec<crate::match_analysis::AnalyzedGame>,
) -> PlayReview {
    let human = session.human_player as usize;
    let final_choice_loss: f64 = games
        .iter()
        .flat_map(|g| &g.decisions)
        .filter(|d| d.player == session.human_player)
        .map(|d| d.equity_loss)
        .sum();
    let delta: f64 = session
        .decisions
        .iter()
        .filter_map(|d| Some(d.first_loss? - d.loss?))
        .sum();
    let first_attempt_loss = (final_choice_loss + delta).max(0.0);
    let first_attempt_count = games
        .iter()
        .flat_map(|g| &g.decisions)
        .filter(|d| d.player == session.human_player)
        .count();
    let native_loss: f64 = games
        .iter()
        .filter_map(|g| g.statistics.as_ref())
        .map(|p| p[human].normalized_equity_loss)
        .sum();
    let native_decisions: i64 = games
        .iter()
        .filter_map(|g| g.statistics.as_ref())
        .map(|p| {
            i64::from(p[human].unforced_checker_moves) + i64::from(p[human].close_cube_decisions)
        })
        .sum();
    let relevance_delta: i64 = session
        .decisions
        .iter()
        .filter_map(|d| Some(i64::from(d.first_rating_decisions?) - i64::from(d.rating_decisions?)))
        .sum();
    let relevant = native_decisions + relevance_delta;
    let has_coverage = games.iter().all(|g| g.statistics.is_some())
        && relevant >= 0
        && relevant <= i64::from(i32::MAX)
        && session.decisions.iter().all(|d| {
            d.committed
                && d.original_action.is_some()
                && d.first_loss.is_some()
                && d.loss.is_some()
                && d.first_rating_decisions.is_some()
                && d.rating_decisions.is_some()
        });
    let assisted_first_attempt_count = session
        .decisions
        .iter()
        .filter(|d| d.attempts.first().is_some_and(|a| a.assisted))
        .count();
    PlayReview {
        analysis_plies: session.analysis_plies,
        games,
        assisted: session.learning_used
            || session.play_mode != PlayMode::Competition
            || session
                .decisions
                .iter()
                .any(|d| d.assisted || d.revealed || d.warning),
        first_attempt_loss,
        final_choice_loss,
        first_attempt_count,
        retry_count: session.decisions.iter().map(|d| d.retry_count).sum(),
        first_attempt_rating: if has_coverage {
            crate::match_analysis::performance_rating_value(
                (native_loss + delta).max(0.0),
                relevant,
            )
        } else {
            None
        },
        first_attempt_rating_decisions: i32::try_from(relevant)
            .ok()
            .filter(|n| *n >= 0)
            .unwrap_or(0),
        first_attempt_rating_method: "gnubg-original-choices-emg500-reached-positions-v1".into(),
        first_attempt_rating_profile: crate::match_analysis::performance_rating_profile(
            session.analysis_version(),
            session.analysis_plies,
        ),
        independent_first_attempt_count: first_attempt_count
            .saturating_sub(assisted_first_attempt_count),
        assisted_first_attempt_count,
    }
}

/// GNUbg eval.c isCloseCubedecision uses native float arithmetic and a
/// literal 1.0, even in match play. Do not classify rounded text equities.
fn native_close_cube(optimal: f64, take: f64) -> bool {
    (optimal as f32 - (take as f32).min(1.0)) < 0.16_f32
}
pub(super) async fn analyze_decision(
    session: &PlaySession,
    path: &str,
) -> anyhow::Result<Vec<PlayAlternative>> {
    let engine = match session.analysis_engine(path)? {
        crate::engines::AnalysisEngine::OpenSage(engine) => {
            return super::open_sage::alternatives(session, &engine).await;
        }
        crate::engines::AnalysisEngine::Gnubg(engine) => engine,
    };
    let directory = tempfile::tempdir()?;
    let mut commands = crate::engine::decision_hint_commands(
        &session.position.position_id,
        &session.position.gnubg_match_id,
        session.analysis_plies,
    )?;
    if !matches!(session.phase, PlayPhase::Move) {
        let native = crate::engine::gnubg_plies(session.analysis_plies)?;
        tokio::fs::write(directory.path().join("cube.py"),format!("import gnubg,json\nv=gnubg.cfevaluate(gnubg.board(),gnubg.cubeinfo(),{{'cubeful':1,'plies':{native},'deterministic':1,'noise':0.0,'prune':1}})\njson.dump(list(v[:4]),open('cube.json','w'),allow_nan=False)\n")).await?;
        commands.push_str("load python cube.py\n");
    }
    commands.push_str("hint 10000\n");
    let output = engine
        .run_with_timeout(
            directory.path(),
            &commands,
            std::time::Duration::from_secs(180),
        )
        .await?;
    if matches!(session.phase, PlayPhase::Move) {
        if session.legal_moves.is_empty() {
            return Ok(vec![PlayAlternative {
                rating_decisions: 0,
                action: "pass".into(),
                move_index: None,
                notation: "pass".into(),
                loss: 0.0,
                equity: None,
                probabilities: None,
            }]);
        }
        let (candidates, _) =
            crate::engine::parse_full_checker_hints(&output, session.analysis_plies)?;
        let best = candidates
            .iter()
            .map(|c| c.equity)
            .fold(f64::NEG_INFINITY, f64::max);
        let mut alternatives = Vec::new();
        for candidate in candidates {
            // The same native formatter produces legal moves and evaluation hints.
            let index = session
                .legal_moves
                .iter()
                .position(|m| m.notation == candidate.notation)
                .context("Evaluated checker move is absent from legal catalog")?;
            alternatives.push(PlayAlternative {
                rating_decisions: i32::from(session.legal_moves.len() > 1),
                action: "move".into(),
                move_index: Some(index),
                notation: candidate.notation,
                loss: crate::match_analysis::normalized_loss(candidate.equity, best),
                equity: Some(candidate.equity),
                probabilities: Some(candidate.probabilities),
            });
        }
        ensure!(
            alternatives.len() == session.legal_moves.len(),
            "Analysis did not cover every legal checker move"
        );
        return Ok(alternatives);
    }
    let crate::engine::CubeEquities {
        no_double,
        take,
        pass,
        ..
    } = crate::engine::parse_cube_equities(&output)?;
    let native_values: Vec<f64> =
        serde_json::from_slice(&tokio::fs::read(directory.path().join("cube.json")).await?)?;
    ensure!(
        native_values.len() == 4 && native_values.iter().all(|v| v.is_finite()),
        "Invalid native cube values"
    );
    let close = native_close_cube(native_values[0], native_values[2]);
    let probabilities = output
        .lines()
        .find_map(|line| {
            let values: Vec<f64> = line
                .split_whitespace()
                .filter_map(|part| part.parse::<f64>().ok())
                .collect();
            (values.len() == 6 && values.iter().all(|v| (0.0..=1.0).contains(v))).then_some(values)
        })
        .context("GNUbg cube probabilities missing")?;
    let probabilities = if matches!(session.phase, PlayPhase::DoubleResponse) {
        vec![
            probabilities[3],
            probabilities[4],
            probabilities[5],
            probabilities[0],
            probabilities[1],
            probabilities[2],
        ]
    } else {
        probabilities
    };
    let values = if matches!(session.phase, PlayPhase::DoubleResponse) {
        vec![("take", -take), ("drop", -pass)]
    } else {
        vec![("roll", no_double), ("double", take.min(pass))]
    };
    let best = values
        .iter()
        .map(|(_, eq)| *eq)
        .fold(f64::NEG_INFINITY, f64::max);
    Ok(values
        .into_iter()
        .map(|(action, equity)| PlayAlternative {
            rating_decisions: i32::from(action != "roll" || close),
            action: action.into(),
            move_index: None,
            notation: if action == "roll" {
                "No double".into()
            } else {
                action.into()
            },
            loss: crate::match_analysis::normalized_loss(equity, best),
            equity: Some(equity),
            probabilities: Some(probabilities.clone()),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session(mode: PlayMode) -> PlaySession {
        serde_json::from_value(serde_json::json!({"id":Uuid::new_v4(),"version":0,"human_player":0,"human_name":"Human","match_length":5,"bearoff_direction":"right","engine":{"id":"gnubg","version":"test","plies":1},"position":{"id":Uuid::new_v4(),"match_uuid":Uuid::new_v4(),"ordinal":0,"game_no":1,"ply":0,"player":0,"dice":[3,1],"board":vec![0;26],"position_id":"","gnubg_match_id":"","notation":""},"phase":"move","can_double":false,"legal_moves":[],"history":[],"scores":[0,0],"game_no":1,"winner":null,"play_mode":mode})).unwrap()
    }
    fn alternatives() -> Vec<PlayAlternative> {
        vec![
            PlayAlternative {
                rating_decisions: 1,
                action: "move".into(),
                move_index: Some(0),
                notation: "8/5".into(),
                loss: 0.0,
                equity: Some(0.25),
                probabilities: Some(vec![0.5; 6]),
            },
            PlayAlternative {
                rating_decisions: 1,
                action: "move".into(),
                move_index: Some(1),
                notation: "6/3".into(),
                loss: 0.08,
                equity: Some(0.17),
                probabilities: Some(vec![0.4; 6]),
            },
        ]
    }
    #[tokio::test]
    #[ignore = "requires native GNUbg"]
    async fn feedback_evaluates_every_legal_move_at_three_plies() {
        let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let mut state = session(PlayMode::Training);
        state.analysis_plies = 3;
        state.engine.version = engine.version().await.unwrap();
        state.position.position_id = "4HPwATDgc/ABMA".into();
        state.position.gnubg_match_id = "cInlAAAAAAAE".into();
        let directory = tempfile::tempdir().unwrap();
        let output=engine.run_with_timeout(directory.path(),"set display off\nset gnubgid 4HPwATDgc/ABMA:cInlAAAAAAAE\nset evaluation chequerplay evaluation plies 0\nhint 10000\n",std::time::Duration::from_secs(30)).await.unwrap();
        state.legal_moves = crate::engine::parse_hints(&output)
            .unwrap()
            .into_iter()
            .map(|c| PlayMove {
                notation: c.notation,
                steps: vec![],
            })
            .collect();
        let alternatives = analyze_decision(&state, engine.binary()).await.unwrap();
        assert_eq!(alternatives.len(), state.legal_moves.len());
        // Each alternative is independently evaluated by a GNUbg-native 2-ply hint
        // whose filter retains all plays; no 0-ply tail may enter the ranking.
        assert!(alternatives.len() > 5);
    }
    #[tokio::test]
    #[ignore = "requires native GNUbg"]
    async fn forced_single_checker_play_keeps_zero_native_denominator_at_three_plies() {
        let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let mut state = session(PlayMode::Coach);
        state.engine.version = engine.version().await.unwrap();
        state.analysis_plies = 3;
        state.position.position_id = "4P8PAAABAAAAAA".into();
        state.position.gnubg_match_id = "MAGnADAAAAAE".into();
        state.legal_moves = vec![PlayMove {
            notation: "1/off".into(),
            steps: vec![],
        }];
        let alternatives = analyze_decision(&state, engine.binary()).await.unwrap();
        assert_eq!(alternatives.len(), 1);
        assert_eq!(alternatives[0].loss, 0.0);
        assert_eq!(alternatives[0].rating_decisions, 0);
    }
    #[tokio::test]
    #[ignore = "requires native GNUbg"]
    async fn original_cube_relevance_and_loss_match_native_game_statistics_at_one_and_three_plies()
    {
        let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let source = "(;GM[6]MI[length:7];AE[a:y]AW[f]AB[s]PL[W];W[21fddc])";
        let imported = crate::import::parse_sgf(source).unwrap();
        for depth in [1, 3] {
            let mut state = session(PlayMode::Training);
            state.phase = PlayPhase::Roll;
            state.can_double = true;
            state.analysis_plies = depth;
            state.engine.version = engine.version().await.unwrap();
            state.position.position_id = imported.positions[0].position_id.clone();
            // The cube decision precedes the rolled checker dice.
            state.position.gnubg_match_id =
                crate::import::ids::hide_dice(&imported.positions[0].match_id).unwrap();
            let feedback = analyze_decision(&state, engine.binary()).await.unwrap();
            let no_double = feedback.iter().find(|a| a.action == "roll").unwrap();
            let double = feedback.iter().find(|a| a.action == "double").unwrap();
            assert_eq!(no_double.rating_decisions, 1);
            assert_eq!(double.rating_decisions, 1);
            assert_eq!(no_double.probabilities.as_ref().unwrap().len(), 6);
            let game = engine.analyze_game(source, 1, depth).await.unwrap();
            let stats = &game.statistics.as_ref().unwrap()[0];
            assert_eq!(stats.unforced_checker_moves, 0);
            assert_eq!(stats.close_cube_decisions, 1);
            let cube = game.decisions.iter().find(|d| d.kind == "cube").unwrap();
            assert!((cube.equity_loss - no_double.loss).abs() < 0.000002);
            // Forced checker choices contribute no denominator even though the
            // no-double at this reached position is a native close decision.
            assert!((stats.normalized_equity_loss - no_double.loss).abs() < 0.000002);
        }
    }
    #[test]
    fn original_rating_uses_native_relevance_and_retains_first_action_denominator() {
        assert!(!native_close_cube(0.3_f32 as f64, 0.14_f32 as f64));
        assert!(native_close_cube(0.299_f32 as f64, 0.14_f32 as f64));
        let mut state = session(PlayMode::Tutor);
        let mut choices = alternatives();
        choices[1].rating_decisions = 0;
        stage_choice(&mut state, "move", Some(1), choices.clone())
            .ok()
            .unwrap();
        state.pending_decision.as_mut().unwrap().action = None;
        stage_choice(&mut state, "move", Some(0), choices)
            .ok()
            .unwrap();
        let pending = state.pending_decision.unwrap();
        assert_eq!(pending.first_rating_decisions, Some(0));
        assert_eq!(pending.rating_decisions, Some(1));
    }
    #[test]
    fn legacy_sessions_resume_competition() {
        let mut value = serde_json::to_value(session(PlayMode::Coach)).unwrap();
        value.as_object_mut().unwrap().remove("play_mode");
        assert_eq!(
            serde_json::from_value::<PlaySession>(value)
                .unwrap()
                .play_mode,
            PlayMode::Competition
        );
    }
    #[test]
    fn tutor_warning_is_neutral_and_first_attempt_survives_retry() {
        let mut state = session(PlayMode::Tutor);
        stage_choice(&mut state, "move", Some(1), alternatives())
            .ok()
            .unwrap();
        let public = present(state.clone());
        let decision = public.pending_decision.unwrap();
        assert!(decision.warning);
        assert!(!decision.revealed);
        assert!(decision.loss.is_none());
        assert!(decision.alternatives.is_empty());
        let pending = state.pending_decision.as_mut().unwrap();
        pending.action = None;
        pending.move_index = None;
        pending.retry_count += 1;
        pending.assisted = true;
        stage_choice(&mut state, "move", Some(0), alternatives())
            .ok()
            .unwrap();
        let pending = state.pending_decision.unwrap();
        assert_eq!(pending.first_loss, Some(0.08));
        assert_eq!(pending.loss, Some(0.0));
        assert!(!pending.attempts[0].assisted);
        assert!(pending.attempts[1].assisted);
    }
    #[test]
    fn coach_returns_deltas_without_absolute_evaluation() {
        let mut state = session(PlayMode::Coach);
        stage_choice(&mut state, "move", Some(1), alternatives())
            .ok()
            .unwrap();
        let pending = present(state).pending_decision.unwrap();
        assert_eq!(pending.loss, Some(0.08));
        assert!(
            pending
                .alternatives
                .iter()
                .all(|a| a.equity.is_none() && a.probabilities.is_none())
        );
    }
    #[test]
    fn mixed_mode_review_keeps_native_competition_segments_and_original_choice_deltas() {
        let mut state = session(PlayMode::Coach);
        state.learning_used = true;
        let cube = vec![
            PlayAlternative {
                rating_decisions: 0,
                action: "roll".into(),
                move_index: None,
                notation: "No double".into(),
                loss: 0.20,
                equity: Some(0.0),
                probabilities: None,
            },
            PlayAlternative {
                rating_decisions: 1,
                action: "double".into(),
                move_index: None,
                notation: "double".into(),
                loss: 0.0,
                equity: Some(0.2),
                probabilities: None,
            },
        ];
        stage_choice(&mut state, "roll", None, cube.clone())
            .ok()
            .unwrap();
        state.pending_decision.as_mut().unwrap().action = None;
        state.pending_decision.as_mut().unwrap().assisted = true;
        stage_choice(&mut state, "double", None, cube).ok().unwrap();
        let mut corrected = state.pending_decision.take().unwrap();
        corrected.committed = true;
        state.decisions.push(corrected);
        let mut hinted = empty_decision(alternatives());
        hinted.assisted = true;
        state.pending_decision = Some(hinted);
        stage_choice(&mut state, "move", Some(0), alternatives())
            .ok()
            .unwrap();
        let mut hinted = state.pending_decision.take().unwrap();
        hinted.committed = true;
        state.decisions.push(hinted);
        state.play_mode = PlayMode::Competition;
        let mut decisions = Vec::new();
        for index in 1..=8 {
            decisions.push(serde_json::from_value(serde_json::json!({"decision_no":index,"checker_no":index,"player":0,"kind":if index==8{"cube"}else{"checker"},"played":"played","best":"best","played_equity":0.0,"best_equity":0.0375,"equity_loss":0.0375,"severity":"none","dice":null,"evaluation_status":"evaluated"})).unwrap());
        }
        let games = vec![crate::match_analysis::AnalyzedGame {
            engine: "test".into(),
            game_no: 1,
            decisions,
            statistics: Some([
                crate::match_analysis::NativePlayerStatistics {
                    total_checker_moves: 7,
                    total_cube_decisions: 1,
                    unforced_checker_moves: 4,
                    close_cube_decisions: 1,
                    normalized_equity_loss: 0.30,
                },
                crate::match_analysis::NativePlayerStatistics {
                    total_checker_moves: 0,
                    total_cube_decisions: 0,
                    unforced_checker_moves: 0,
                    close_cube_decisions: 0,
                    normalized_equity_loss: 0.0,
                },
            ]),
        }];
        let review = build_review(&state, games.clone());
        assert!((review.final_choice_loss - 0.30).abs() < 1e-10);
        assert!((review.first_attempt_loss - 0.50).abs() < 1e-10);
        assert_eq!(review.first_attempt_rating_decisions, 4);
        assert!((review.first_attempt_rating.unwrap() - 62.5).abs() < 1e-10);
        assert_eq!(review.first_attempt_count, 8);
        assert_eq!(review.assisted_first_attempt_count, 1);
        assert_eq!(review.independent_first_attempt_count, 7);
        assert!(review.assisted);
        state.decisions[0].first_rating_decisions = None;
        assert!(
            build_review(&state, games.clone())
                .first_attempt_rating
                .is_none()
        );
        state.decisions.clear();
        state.learning_used = false;
        let pure = build_review(&state, games);
        assert!((pure.first_attempt_rating.unwrap() - 30.0).abs() < 1e-10);
        assert_eq!(pure.first_attempt_rating_decisions, 5);
        assert_eq!(pure.independent_first_attempt_count, 8);
        assert!(!pure.assisted);
    }
    #[test]
    fn coach_roll_warning_uses_the_default_error_boundary() {
        let mut state = session(PlayMode::Coach);
        for (loss, pauses) in [(0.019999, false), (0.02, true)] {
            state.pending_decision = None;
            let roll = PlayAlternative {
                rating_decisions: 1,
                action: "roll".into(),
                move_index: None,
                notation: "No double".into(),
                loss,
                equity: None,
                probabilities: None,
            };
            assert!(stage_choice(&mut state, "roll", None, vec![roll]).is_ok());
            assert_eq!(should_pause_for_feedback(&state, &PlayAction::Roll), pauses);
        }
    }

    #[test]
    fn coach_pauses_at_the_saved_checker_error_boundary() {
        let mut state = session(PlayMode::Coach);
        state.error_thresholds.error = 0.03;
        state.tutor_threshold = 0.05;
        for (loss, pauses) in [(0.0, false), (0.029999, false), (0.03, true)] {
            state.pending_decision = None;
            let mut choices = alternatives();
            choices[1].loss = loss;
            assert!(stage_choice(&mut state, "move", Some(1), choices).is_ok());
            assert_eq!(should_pause_for_feedback(&state, &PlayAction::Move), pauses);
        }
    }

    #[test]
    fn tutor_threshold_does_not_change_severity() {
        let mut state = session(PlayMode::Tutor);
        state.tutor_threshold = 0.12;
        stage_choice(&mut state, "move", Some(1), alternatives())
            .ok()
            .unwrap();
        let pending = state.pending_decision.unwrap();
        assert!(!pending.warning);
        assert_eq!(pending.severity.as_deref(), Some("blunder"));
    }
}
