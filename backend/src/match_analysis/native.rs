use super::*;

impl Engine {
    pub async fn analyze_game(&self, sgf: &str, game_no: i32, plies: i32) -> Result<AnalyzedGame> {
        crate::engine::with_shared_access(self.analyze_game_with_limit(
            sgf,
            game_no,
            plies,
            Some(std::time::Duration::from_secs(600)),
        ))
        .await
    }

    pub(crate) async fn analyze_game_with_limit(
        &self,
        sgf: &str,
        game_no: i32,
        plies: i32,
        timeout: Option<std::time::Duration>,
    ) -> Result<AnalyzedGame> {
        let native = crate::engine::gnubg_plies(plies)?;
        let source = sgf.to_owned();
        let (source, input) = tokio::task::spawn_blocking(move || {
            let source = AnalysisSource::for_game(&source, game_no)?;
            let input = analysis_input(&source);
            anyhow::Ok((source, input))
        })
        .await??;
        let temp = tempfile::tempdir()?;
        tokio::fs::write(temp.path().join("input.sgf"), input).await?;
        let mut commands = String::from(
            "set confirm default yes\nset confirm new no\nset display off\nset analysis moves on\nset analysis cube on\nset analysis luck off\nset analysis player 0 analyse on\nset analysis player 1 analyse on\nset output mwc off\nset output digits 6\nset export moves number 10000\nset export include statistics on\n",
        );
        let cube_enabled = source.contexts[(game_no - 1) as usize].cube_enabled;
        let cubeful = if cube_enabled { "on" } else { "off" };
        if !cube_enabled {
            commands.push_str("set analysis cube off\nset cube use off\n");
        }
        for kind in ["chequerplay", "cubedecision"] {
            commands.push_str(&format!("set analysis {kind} type evaluation\nset analysis {kind} evaluation plies {native}\nset analysis {kind} evaluation cubeful {cubeful}\nset analysis {kind} evaluation deterministic on\nset analysis {kind} evaluation noise 0\n"));
        }
        self.append_move_filter(&mut commands, "analysis", plies);
        for mark in ["verybad", "bad", "doubtful", "unmarked"] {
            commands.push_str(&format!(
                "set export moves display {mark} on\nset export cube display {mark} on\n"
            ));
        }
        for mark in ["actual", "close", "missed"] {
            commands.push_str(&format!("set export cube display {mark} on\n"));
        }
        commands.push_str("load match input.sgf\nfirst game\n");
        if game_no > 1 {
            commands.push_str(&format!("next game {}\n", game_no - 1));
        }
        commands.push_str("analyse clear game\nanalyse game\nsave game analyzed.sgf\nexport game text analyzed.txt\n");
        let output = self.run_with_limit(temp.path(), &commands, timeout).await?;
        for line in output.lines() {
            let lower = line.trim().to_ascii_lowercase();
            ensure!(
                !["error", "warning", "unknown", "illegal", "invalid"]
                    .iter()
                    .any(|p| lower.starts_with(p)),
                "GNUbg rejected game analysis: {line}"
            );
        }
        let native = read_analysis_artifact(&temp.path().join("analyzed.sgf")).await?;
        let export = read_analysis_artifact(&temp.path().join("analyzed.txt")).await?;
        let (decisions, statistics) = tokio::task::spawn_blocking(move || {
            let decisions = parse_source_game(&source, game_no, &native, &export)?;
            let statistics = parse_native_statistics(&export, game_no, &decisions, cube_enabled);
            anyhow::Ok((decisions, statistics))
        })
        .await??;
        let engine = output
            .lines()
            .find(|l| l.starts_with("GNU Backgammon "))
            .context("GNUbg engine version missing")?
            .to_owned();
        Ok(AnalyzedGame {
            cube_enabled,
            engine,
            game_no,
            decisions,
            statistics,
        })
    }
}

pub(super) async fn read_analysis_artifact(path: &std::path::Path) -> Result<String> {
    const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
    let file = tokio::fs::File::open(path)
        .await
        .context("GNUbg analysis artifact is missing")?;
    ensure!(
        file.metadata().await?.len() <= MAX_ARTIFACT_BYTES,
        "GNUbg analysis artifact exceeds 64 MiB limit"
    );
    let mut text = String::new();
    file.take(MAX_ARTIFACT_BYTES + 1)
        .read_to_string(&mut text)
        .await?;
    ensure!(
        text.len() as u64 <= MAX_ARTIFACT_BYTES,
        "GNUbg analysis artifact exceeds 64 MiB limit"
    );
    Ok(text)
}

pub(super) fn retain_checker_paths<'a>(
    candidates: impl Iterator<Item = &'a mut Candidate>,
    position: &ImportedPosition,
    analysis: &[String],
) {
    let Ok(before) = <[i32; 26]>::try_from(position.board.as_slice()) else {
        return;
    };
    let [first, second] = position.dice;
    for candidate in candidates {
        // A[played-index][rank-1 move and evaluation][rank-2 ...].
        let Some(coordinates) = usize::try_from(candidate.rank)
            .ok()
            .and_then(|rank| analysis.get(rank))
            .and_then(|value| value.split_whitespace().next())
            .filter(|value| value.len() <= 8)
        else {
            continue;
        };
        let mut after = before;
        if let Ok((_, path)) = crate::import::sgf::play(
            &mut after,
            position.player,
            &format!("{first}{second}{coordinates}"),
        ) && crate::move_assessments::after_checker_play(
            before,
            position.player,
            position.dice,
            &candidate.notation,
        ) == Some(after)
        {
            candidate.path = Some(path);
        }
    }
}

pub(crate) async fn retain_hint_paths(
    hint_sgf: &std::path::Path,
    position_id: &str,
    candidates: &mut [Candidate],
) {
    // Some GNUbg builds and decisions only expose textual hints.
    let Ok(sgf) = read_analysis_artifact(hint_sgf).await else {
        return;
    };
    let Ok((matched, _)) = crate::import::sgf::parse_native_sgf_with_context(&sgf) else {
        return;
    };
    let Some(position) = matched
        .positions
        .last()
        .filter(|p| p.position_id == position_id)
    else {
        return;
    };
    let Ok(trees) = parse_trees(&sgf) else {
        return;
    };
    if let Some(analysis) = trees
        .last()
        .and_then(|nodes| nodes.last())
        .and_then(|node| node.get("A"))
    {
        retain_checker_paths(candidates.iter_mut(), position, analysis);
    }
}

/// The played match of one native game analysis, parsed once for both the
/// GNUbg input and the check of GNUbg's result.
pub(super) struct AnalysisSource {
    pub(super) trees: Vec<Vec<Node>>,
    pub(super) imported: ImportedMatch,
    pub(super) contexts: Vec<GameContext>,
}

impl AnalysisSource {
    /// Validate the entire played match before handing it to the native loader,
    /// which can otherwise continue with a valid prefix of a malformed game.
    pub(super) fn for_game(sgf: &str, game_no: i32) -> Result<Self> {
        let trees = parse_trees(sgf)?;
        ensure!(
            (1..=tree_game_count(&trees)?).contains(&game_no),
            "Game number outside match"
        );
        let (imported, contexts) = parse_importable_sgf_with_context(sgf)?;
        Ok(Self {
            trees,
            imported,
            contexts,
        })
    }
}

/// Keep all game state, but exclude imported analysis and arbitrary annotations.
/// Fixed names also prevent player-name newlines from resembling export headers.
pub(super) fn analysis_input(source: &AnalysisSource) -> String {
    let mut out = String::new();
    for (game_index, nodes) in source.trees.iter().enumerate() {
        let context = &source.contexts[game_index];
        out.push('(');
        for (index, node) in nodes.iter().enumerate() {
            if index == 0 {
                out.push(';');
                for key in ["FF", "GM", "CA", "RU", "RE"] {
                    if let Some(values) = node.get(key) {
                        if key == "RU" && context.crawford {
                            // Native GNUbg recognizes CrawfordGame only together
                            // with its Crawford rule flag; strict SGF accepts either.
                            let mut rules = values[0].split(':').map(str::trim).collect::<Vec<_>>();
                            if !rules.contains(&"Crawford") {
                                rules.insert(0, "Crawford");
                            }
                            write_property(&mut out, key, &[rules.join(":")]);
                        } else {
                            write_property(&mut out, key, values);
                        }
                    }
                }
                // The importer inherits omitted scores/length, whereas GNUbg
                // defaults a missing MI to a fresh money session.
                write_property(
                    &mut out,
                    "MI",
                    &[
                        format!("length:{}", context.length),
                        format!("game:{game_index}"),
                        format!("ws:{}", context.scores[0]),
                        format!("bs:{}", context.scores[1]),
                    ],
                );
                out.push_str("PW[Player White]PB[Player Black]");
            }
            // GNUbg ignores setup properties in the game-info root node. Emit
            // setup before the move, and translate standard CO to GNUbg's CP.
            if ["CV", "CO", "CP", "PL", "AE", "AW", "AB"]
                .iter()
                .any(|key| node.contains_key(*key))
            {
                out.push(';');
                if let Some(values) = node.get("CV") {
                    write_property(&mut out, "CV", values);
                }
                let owner = node.get("CO").or_else(|| {
                    node.get("CP")
                        .filter(|v| v.len() == 1 && ["c", "w", "b"].contains(&v[0].as_str()))
                });
                if let Some(values) = owner {
                    if values.as_slice() == ["n"] {
                        write_property(&mut out, "CP", &["c".into()]);
                    } else {
                        write_property(&mut out, "CP", values);
                    }
                }
                for key in ["PL", "AE", "AW", "AB"] {
                    if let Some(values) = node.get(key) {
                        write_property(&mut out, key, values);
                    }
                }
            }
            for key in ["W", "B"] {
                if let Some(values) = node.get(key) {
                    out.push(';');
                    write_property(&mut out, key, values);
                }
            }
        }
        out.push_str(")\n");
    }
    out
}
