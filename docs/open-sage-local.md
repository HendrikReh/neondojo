# Local NeonGammon installation and compatibility evidence

NeonGammon ([HendrikReh/neongammon](https://github.com/HendrikReh/neongammon)) is the project's own fork of Open Sage (`markbgsage/bgsage`). Since version 3.0.0, the Python package is called `neongammon`. NeonDojo uses `neongammon==3.1.0` for the named opponent presets. Version 3.0.0 remains supported for analysis and ordinary play using existing snapshots. In NeonDojo the engine is called "NeonGammon"; its internal identifier remains `bgsage`. The original Open Sage wheel is not a prerequisite for any installation or test.

This page is divided into the current state and the history:

- **Current:** [Installation](#reproducible-installation), [Verified scope](#verified-scope-and-source-identity), [Use in NeonDojo](#use-in-neondojo), [Tests with a real installation](#tests-with-a-real-installation), [NeonGammon 3.1.0 – November 21, 2026](#neongammon-310--november-21-2026) and [Engine contract and adapter](#engine-contract-and-adapter).
- **History:** The dated sections under [Earlier Open Sage builds](#earlier-open-sage-builds) document the `bgsage` 2.0.x builds with their measurements and are kept unchanged as a record.

This is local evidence for these builds and reference positions, not a statement about playing strength or general GNUbg equivalence.

## Reproducible installation

Prerequisites: a native ARM64 Python 3.12 interpreter, macOS 14 or later (NumPy wheel), and enough local storage. No Rosetta interpreter and no source build. NeonGammon is published only as a GitHub release asset, not on PyPI. The lock [`tools/open-sage/requirements-macos-arm64.lock`](../tools/open-sage/requirements-macos-arm64.lock) therefore points to the release wheel and contains the SHA256 hashes of the two ARM64 wheels (`neongammon` 3.1.0 and NumPy 2.5.4). Models and bear-off data are included in the NeonGammon wheel.

**1. Install the environment.** From the repository root, with an installation directory of your choice outside the repository:

```sh
python3.12 -m venv "$HOME/Library/Application Support/NeonDojo/neongammon-venv"
"$HOME/Library/Application Support/NeonDojo/neongammon-venv/bin/python" -m pip install \
  --only-binary=:all: --require-hashes \
  -r tools/open-sage/requirements-macos-arm64.lock
```

Offline: download both wheels unchanged beforehand (`gh release download v3.1.0 --repo HendrikReh/neongammon --pattern '*cp312*'` and NumPy 2.5.4) and use a lock with `neongammon==3.1.0` and the same hashes. The local installation under `local-data/neongammon-3.1.0/` contains exactly this combination (`wheels/`, `requirements.lock`):

```sh
python3.12 -m venv /path/to/new-venv
/path/to/new-venv/bin/python -m pip install --no-index --only-binary=:all: --require-hashes \
  --find-links local-data/neongammon-3.1.0/wheels \
  -r local-data/neongammon-3.1.0/requirements.lock
```

**2. Verify the installation.** The probe's Python tests do not need a NeonGammon installation; the probe itself runs with the new environment's interpreter:

```sh
python3 -m unittest discover -s tools -p test_probe_open_sage.py
"$HOME/Library/Application Support/NeonDojo/neongammon-venv/bin/python" \
  tools/probe_open_sage.py --levels 1ply 2ply --output /tmp/neongammon-1ply-2ply.json
```

Exit code 0 means that **all requested** checks passed. Missing dependencies, a wrong architecture or Python version, an unsupported package version, missing, empty or modified assets, native crashes, timeouts and failed reference cases produce an error report and exit code 1. `--levels` accepts `1ply` and `2ply`. The time limit defaults to 300 seconds and can be changed with `--timeout`; when it is exceeded, the started worker is terminated. The probe installs nothing and downloads nothing.

**3. Register it in NeonDojo:** see [Use in NeonDojo](#use-in-neondojo).

## Verified scope and source identity

The report for the current build is [probe-neongammon-3.1.0-macos-arm64.json](../tools/open-sage/probe-neongammon-3.1.0-macos-arm64.json). It contains the platform, package versions, production model `stage11`, strategy `backgame_pair_phased`, 24 model slots, and the SHA256 of every Python module, the native library and all assets. Files are checked against the SHA256 entries of the installed wheel's RECORD. The installation lock protects the original wheels; a RECORD that was tampered with together with the files cannot be detected by this local integrity check alone. The reports for the earlier builds are [probe-macos-arm64-2ply.json](../tools/open-sage/probe-macos-arm64-2ply.json) and [probe-macos-arm64.json](../tools/open-sage/probe-macos-arm64.json) (both `bgsage` 2.0.20260907, November 12, 2026, see [history](#limitation-of-the-original-wheel-1-ply-thread-budget)).

For each requested level, the functional check covers:

- Opening 31: legal candidates and best move `8/5 6/5`.
- Both perspectives, asymmetric bars, legal bar entry with a hit, and notation.
- Money game with and without Jacoby, cube owned by the player, by the opponent or centered, beavers explicitly off.
- Asymmetric match score, Crawford from both perspectives, and post-Crawford.
- Forced pass, bear-off and post-move evaluation.
- Terminal single wins and losses in money, match and Crawford play return probability 1/0 and equity +1/-1. Negative match equity rules out a direct interpretation as MWC; further domain conversions require their own references.
- Safely won one-checker position: money with cube 2 returns No Double ≈1, Double/Take ≈2 and Double/Pass ≈1 in units of the cube **before** the offer. The response perspective negates the original DT/DP values (Take −2, Pass −1); the board is not flipped and evaluated again for this. At 1-away/3-away, the match values for cube 1 and cube 2 each saturate at ≈1. The probe does not check an unstable double recommendation when the No Double and Pass values are equal.
- `forced_played_move_2ply`: A played move that the move filter does not promote to full depth is actually evaluated at 2-ply through `force_boards`. Since 2.0.20261119, the check requires the played move at full depth, no longer every candidate.
- Thread budget per level with 2 and 4 requested threads (see [NeonGammon 3.0.0](#neongammon-300--november-20-2026)).
- Finite equities and probabilities, and nested gammon/backgammon bounds, for all checked results.

The regular Python tests need neither NumPy nor NeonGammon and check in particular error handling, hash mismatches, process timeouts, the network block and ignored thread limits.

## Use in NeonDojo

1. Install the NeonGammon environment as described above. For this local deployment, it lives permanently under `local-data/neongammon-3.1.0/venv` in the main repository; `local-data/` is excluded from Git. Packages and weights are not copied into the source code.
2. Build and start the backend and frontend as described in the [README](../README.md#restart-neondojo-locally-on-port-8787). No separate migration is needed: the `engine_installations` table with its `bgsage` record is part of the [schema baseline](development/architecture.md#database-schema) `0001_schema.sql` (formerly migration 0041). Database and browser tests use their own test databases.
3. Under **Settings → Analysis → Engines on this computer → Local NeonGammon installation**, enter the absolute path to the venv interpreter as **Python path** and choose **Verify and save**. The backend checks package and model integrity and runs a real evaluation before the engine becomes selectable. Then choose the analysis engine and the engine for future games separately and apply the draft.
4. Under **Settings → Analysis → Engines on this computer**, **NeonGammon threads for analysis** and **NeonGammon threads for playing** can each be set from 2 up to the number of logical CPUs. With **Default**, NeonGammon uses 4 threads for analyses and engine duels and 2 for playing. New sessions, jobs and duel series store the value in effect. The worker checks the configured limit with a real position evaluation; the serial cubeful 1-ply checker path stays serial.
5. Under **Extras → Benchmark**, set the engine and depth independently. The NeonGammon reference protocols always use 4 threads for analysis and 2 for playing decisions, regardless of these settings.

The sidecar uses `neongammon.DifficultyPolicy` from the verified 3.1.0 wheel.
Preset selections and decisions apply to live NeonGammon opponents. New Full
Strength matches use NeonDojo's verified 3-ply checker search and the wheel
policy's 2-ply cube decisions. A saved named-preset match pinned to 3.0.0
requires the earlier NeonDojo build with the bundled policy. [Opponent presets](reference/playing.md#neongammon-difficulty-presets)

The application process stays fully local. The sidecar has no download path; installation and updates are explicitly separate steps. Unknown or modified models, a missing Python environment and engine crashes produce errors, not substitute evaluations. NeonDojo never falls back to another engine on its own.

### Five-point preset PR benchmark

On November 21, 2026, the Apple M4 Pro validation ran 100 simulated five-point
matches per preset against NeonDojo's Full Strength with 3-ply checker play and
2-ply cube decisions. The bots used the bundled policies and the installed
NeonGammon 3.0.0 Stage 11 weights. Each preset used match seeds 720000–720099,
alternated sides and sampled each turn with 22% probability for scoring at
Stage 11 3-ply. Each benchmark worker used one engine thread.

| Preset | PR | 95% interval | Counted decisions | Match wins |
|--------|---:|-------------:|------------------:|-----------:|
| Beginner | 21.99 | 20.31–23.77 | 1,125 | 10/100 |
| Casual | 16.17 | 14.83–17.55 | 1,232 | 22/100 |
| Intermediate | 8.11 | 7.39–8.91 | 1,430 | 34/100 |

The simulation and scoring use NeonGammon's `calibrate_difficulty.py` validation
at `6e285eb`, with the then-bundled Full Strength policy as the opponent. PR
pools equity loss across counted checker and cube decisions. The 95% intervals
come from 4,000 bootstrap resamples of whole matches. The chooser rounds the
measured PR to one decimal place.

### Full Strength rollout PR

Full Strength measured **0.975 PR** (95% interval **0.890–1.069**) on the
NeonGammon five-point rollout benchmark.
The score pools 6,610 checker choices at 3-ply and 302 cube choices at 2-ply
across archived positions from 130 matches. The choices were scored using the
same full-rollout equities as the comparison of 2-ply and 3-ply. For checker
play, the reference best move and the 2-ply and 3-ply picks all had to have
full rollout values. The 95% interval comes from resampling whole matches. The
five-point chooser rounds this result to **≈ 1.0 PR**. [NeonGammon rollout benchmark](https://github.com/HendrikReh/neongammon/blob/6e285eb/XG_COMPARISON.md#match-play)

## Tests with a real installation

`NEONDOJO_SAGE_PYTHON` and `OPEN_SAGE_TEST_PYTHON` expect the interpreter of the NeonGammon environment (locally `local-data/neongammon-3.1.0/venv/bin/python`). The regular Python and Rust unit tests need no installation. Tests with a database run only against an isolated PostgreSQL test server, see [PostgreSQL and GNUbg integration tests](development/testing.md#postgresql-and-gnubg-integration-tests); SQLx creates its own test databases there.

```sh
python3 -m unittest discover -s tools -p 'test_*open_sage*.py'
cargo test -p neondojo engines:: --lib
NEONDOJO_SAGE_PYTHON=/path/to/venv/bin/python cargo test -p neondojo --test open_sage -- --ignored --test-threads=1
NEONDOJO_SAGE_PYTHON=/path/to/venv/bin/python cargo test -p neondojo sage_executes_reference_checker_turns_using_gnubg_rules --lib -- --ignored --test-threads=1
OPEN_SAGE_TEST_PYTHON=/path/to/venv/bin/python cargo test -p neondojo --test open_sage_settings -- --ignored --test-threads=1
OPEN_SAGE_TEST_PYTHON=/path/to/venv/bin/python cargo test -p neondojo --test open_sage_play -- --ignored --test-threads=1
NEONDOJO_SAGE_PYTHON=/path/to/venv/bin/python npm run test:e2e --prefix web -- e2e/open-sage-native.spec.ts
```

The test targets run individually and with `--test-threads=1`. A combined run of `cargo test --workspace -- --include-ignored` with the Sage variables set makes the process tests in `engines::process` fail, because they share the global worker with the real engine tests. The browser test skips itself without `NEONDOJO_SAGE_PYTHON` and uses the Playwright harness's own database and backend ([Browser tests](development/testing.md#browser-tests)).

The probe and the worker reject earlier `bgsage` builds already at the version check; no test needs the original wheel or an earlier fork.

## NeonGammon 3.1.0 – November 21, 2026

The [official CPython 3.12 arm64 wheel](https://github.com/HendrikReh/neongammon/releases/tag/v3.1.0)
comes from tag commit `04e0bb7`. Its SHA256 is pinned in the installation
lock. The verified wheel with NumPy 2.5.4 has runtime fingerprint
`26e7701c27c6c5e0cd3cc864768e27675f0d72149c2ad545ffb0c5293ff705d5`.
The 1-ply and 2-ply checks passed and Extras → Versions identifies the source
revision. The probe and Rust runtime keep 3.0.0 in their supported list for
older snapshots.

## NeonGammon 3.0.0 – November 20, 2026

On November 20, 2026, NeonDojo switched to release [v3.0.0](https://github.com/HendrikReh/neongammon/releases/tag/v3.0.0) (tag commit `cbfcb01`). The installed wheel is the official cp312 wheel from the release assets, SHA256 `4ed211b7…`, checked against the digest from the release API, with NumPy 2.5.4 under uv CPython 3.12.13 in `local-data/neongammon-3.0.0/venv`. Wheels, lock and provenance are in the same folder (`wheels/`, `requirements.lock`, `provenance.json`, `probe-1ply-2ply.json`). The previous installation `local-data/open-sage-fork-2.0.20261119/` remains in place unchanged, but NeonDojo no longer accepts it.

The supported package versions are listed in `SUPPORTED_VERSIONS` in `tools/probe_open_sage.py` (sidecar and probe). `SUPPORTED_SAGE_PACKAGE_VERSIONS` in `backend/src/engines/mod.rs` must contain the same list; a unit test compares the two. At that release, both listed only `3.0.0`.

- **Package.** The worker and the probe now import only `neongammon` and check the wheel RECORD under `neongammon/`. Models and the bear-off database are unchanged (production model `stage11`, strategy `backgame_pair_phased`, 24 model slots). Because the package files and the manifest change, the runtime fingerprint is new: `621c3554…`. **Extras → Versions** recognizes the release by this fingerprint and shows **NeonGammon source** `HendrikReh/neongammon` with the tag commit.
- **Thread budget.** 3.0.0 evaluates cubeful checker moves from 2-ply upward in the native routine `bgbot_cpp.checker_play_nply` and passes the budget there as `n_threads`; no Python pool is created anymore. The previous check, which observed only `ThreadPoolExecutor`, therefore rejected 2-ply. The probe and the worker now share `observe_thread_budgets`, which additionally observes this argument. Both observers delegate unchanged to the real implementation. Observed: 2-ply with 2 or 4 threads yields `[2]` or `[4]`, respectively; 1-ply stays serial without a pool. What is measured is the budget passed, not a count of concurrently running native threads.
- **Probe.** All checks passed at 1- and 2-ply ([report](../tools/open-sage/probe-neongammon-3.0.0-macos-arm64.json)). Opening 31 at 2-ply: 4.0–4.8 ms warm with 2 or 4 threads; this applies only to this single position, not to game throughput.
- **Candidate scope.** As in 2.0.20261119, 2-ply searches only the survivors of the move filter at full depth; for opening 63 that is 3 of 14 moves, while 4-ply evaluates all 14. Both versions produce the same split here. The native test `native_opening_analysis_retains_all_ranked_checker_candidates` still expected 14 candidates at 2-ply and therefore already failed with 2.0.20261119; it now expects 5 at 2-ply (filter plus played move) and 14 at 4-ply.
- **Values.** Equality of the evaluations with 2.0.20261119 was not checked in general. According to the release notes, single and batch checker play return identical lists; the bear-off cache fix can change individual values when evaluations with and without the bear-off database share the same strategy.

Verified on November 20, 2026 with `NEONDOJO_SAGE_PYTHON` and `OPEN_SAGE_TEST_PYTHON` pointing to this installation:

- Ignored library tests in `engines::`, `play::open_sage`, `engine_matches::` and `practice::evaluation`: 15 passed.
- `--test open_sage`: 2 passed; `--test open_sage_settings`: 7 passed; `--test open_sage_play`: 4 passed.
- `e2e/open-sage-native.spec.ts`: 2 passed, including 4-ply for one game and for the whole match.

## Engine contract and adapter

The following sections describe the current behavior. Where a finding was made on an earlier build, this is stated.

### Board and pass contract

NeonGammon uses 26 entries: points 1–24 with positive values for own checkers and negative values for opposing checkers; the own direction of movement is downward. Index 25 is the own bar, index 0 the opponent's bar. **Both bar values are non-negative.** `flip_board` swaps the bars without changing their signs and mirrors and negates the points.

Contrary to the Python docstring of the checked build 2.0.20260907, `possible_moves` returns a list containing the **unchanged board** for a blocked roll, not an empty list. `checker_play` evaluates this pass candidate. The public notation function returns `""` for it. The probe check `forced_pass_post_move` requires this behavior and also passes with 3.0.0. The adapter must recognize the explicit pass and must not present a missing analysis value as a zero loss.

### Offline check and measurement limits

Before every engine import, a Python audit hook blocks socket connections and DNS; a separate subprocess test confirms the block. The Python hook cannot intercept direct native network system calls. An attempt on November 12, 2026 to forbid `network*` system-wide with macOS `sandbox-exec` already failed when applying the sandbox profile, with `Operation not permitted`. Therefore **no native OS network block is claimed**.

The JSON reports contain the fresh process start including all checks, the import time, the first analyzer initialization, and three warm opening-31 evaluations each after a warm-up with 2/4 requested threads. The OS file caches were not flushed. 2-ply times apply only to this single position with the default move filter; they make no claim about game throughput or UI latency. RSS is the high-water value of the entire worker process across imports, hash checks and all analyzers created in sequence, not a per-request memory value.

The probe does not modify application tables, user games or engine sources.

### Application adapter and numerical limits

The Rust adapter bundles a local JSON sidecar. One worker for playing and one for all other requests are reused per active, immutable profile. Each slot has its own lock: a bot move does not wait for a running analysis block, and while both compute, the playing and analysis threads share the CPU. Analysis blocks, other requests, installation checks and restarts stay serial on the general slot. All slots are additionally under the shared engine access guard, through which benchmarks exclude all other engine work. The deadline of a single request includes waiting for its worker. Package, RECORD and model identity are checked before and after each single request; complete game analyses use the atomic blocks described below. Profile changes and explicit benchmark restarts terminate and reap the previous process. Timeouts, cancellation, malformed protocol frames and invalid typed results discard the worker.

In the full WC2014 reference game 5, the float32 computation of a bear-off variation returned `1.0000000074505806` as the win probability and `-7.450580596923828e-09` as the probability of losing a gammon/backgammon. The adapter clamps only such finite deviations of up to `1e-7` (smaller than a float32 epsilon) from the endpoints 0/1 to the respective endpoint. Larger deviations, non-finite values and inconsistent nested probabilities remain errors. Equities are not changed by this. The independent diagnostic probe keeps its strict check by default.

When playing, NeonGammon decides based on evaluated resulting boards; the notation actually executed comes from GNUbg's catalog of legal moves. GNUbg runs both players as humans in external mode and manages rules, dice, cube and scores. The bot, analysis and rules engines are recorded separately in the session. Double/take/pass consistently uses the context of the original doubler; responses negate the doubler's take/pass equities. For an offered resignation, NeonGammon compares against a terminal position of the same win level in the unchanged match/cube context; GNUbg then executes the explicit acceptance or rejection. NeonGammon does not provide GNUbg PR statistics.

On the last bear-off move, NeonGammon can offer a premature final board even though GNUbg requires using further playable dice. In this special case, NeonDojo uses the best-ranked common final board and stores the executed GNUbg move notation.

### Performance and integrity boundaries

For NeonGammon, the benchmarks use the protocols `bgsage-match-v3-{N}ply-4threads` (analysis) and `bgsage-checker-turn-v3-{N}ply-2threads-gnubg-rules` (play turn); `{N}` is the selected depth. The protocol explicitly restarts the NeonGammon worker, measures its initialization separately and adds it once to the total time. The worker is then reused within that run. The analysis time covers all ten reference games and 460 checker moves with their cube decisions; the play latency covers eleven fixed checker turns with GNUbg rules processes. Several fully compatible runs yield the median and range; a single measurement is not general evidence of speed. Historical results remain unchanged and are not aggregated with new protocols.

A complete identity check hashes each file only once within that check. The next check reads all files again; manifest, fingerprint and existing installation snapshots stay compatible. A game analysis holds the worker exclusively and fully checks the installation, RECORD and weights before and after the game. Only after the final check may the complete block be returned and stored. Errors, timeouts or cancellation discard the block and terminate the worker. Single positions keep their existing check boundaries.

For a NeonGammon play action, the isolated GNUbg rules process is kept between inspection and execution and during the bounded chain of bot actions. Each subsequent step checks the exact GNUbg ID. GNUbg still produces rules, dice and SGF; the revision and the final SGF/snapshot are stored atomically. The process is terminated before storing; on errors, the previously stored game state remains. Log lines, artifacts, runtime and the number of actions are bounded.

For direct GNUbg binaries, the rules adapter reads the version message from the running process. It checks the resolved path and SHA-256 of the binary before startup and after the process exits cleanly. For executable wrappers, the separate version check before and after the action is still required.

### Warm-up for games

When an active NeonGammon game is created, opened or resumed, NeonDojo can load its stored bot profile in the background. The HTTP response does not wait for the sidecar. At most one warm-up runs at a time. A move still uses its stored snapshot and starts a verified worker itself when needed.

An analysis or installation check does not displace the playing process. Changes to the stored playing profile require a new process for that profile. Benchmark runs still start their own worker cold. The checks of artifacts, thread limits, requests, responses and game states apply as before.

### 3- and 4-ply analysis and duels

In the options of **Analyze game** (Analysis header) or **Analyze entire match**, choose NeonGammon as the **Analysis engine** and 3- or 4-ply as the **Analysis depth**. Each job checks and stores the engine, depth and model fingerprint. 3- and 4-ply are also allowed as a saved analysis default and for engine duels (formerly migration 0047, now part of the schema baseline). The sidecar controls the thread limit, the installation identity and the evaluation depth reported by NeonGammon. The results apply to the requested profile and the stored model fingerprint. The manual live playing depths are 1- and 2-ply. The Full Strength preset pins 3-ply for checker moves in new matches.

On November 13, 2026, the fork installed at the time, `5b146c7d86fa965f2185e3871f3c56df030fd76f`, was checked for both higher levels with a real checker and cube evaluation in a 5-point context. For 3.0.0, the browser test with 4-ply for one game and for the whole match passed on November 20, 2026.

## Earlier Open Sage builds

The following sections are historical records of the `bgsage` 2.0.x builds from November 12 to 19, 2026. Paths, commands, numbers and names (Open Sage, BG Tutor, `local-data/bin/bgtutor`, migration numbers before the schema consolidation) reflect the state at that time. The current probe no longer accepts these builds; the commands in these sections can therefore no longer be reproduced unchanged.

### Limitation of the original wheel: 1-ply thread budget

As of November 12, 2026, original PyPI wheel `bgsage` 2.0.20260907. The [complete 1-/2-ply report](../tools/open-sage/probe-macos-arm64.json) intentionally **failed**; the [2-ply report](../tools/open-sage/probe-macos-arm64-2ply.json) passed. With `BgBotAnalyzer(eval_level='1ply', parallel_threads=2)` or `4`, the cubeful checker path created a pool with limit **10** on the test machine. The public parameter was not passed on when `_OnePlyAnalyzer` was created; `_CubefulAnalyzer.checker_play_analytics` fell back to `max(2, os.cpu_count())`. `BGBOT_MULTIPLY_THREADS` did not limit this particular path either. No alternative public 1-ply limit was found in this build.

For this, the probe observed the real `ThreadPoolExecutor` constructor and delegated to it unchanged. It changed neither the pool size nor the computation. This measures the configured candidate pool limit, not all concurrently running native threads. At 2-ply, the requested limits 2 and 4 were respected. Because of this defect, the 1-ply measurements in the complete report actually have pool limit 10 and are not a 2-/4-thread comparison. 1-ply remained disabled until the [patched fork](#patched-local-fork-and-1-ply--november-12-2026); private engine fields were not modified. 3-/4-ply and rollouts were not enabled or measured at that time.

Reproduction command at the time (exit code 1 expected):

```sh
"$HOME/Library/Application Support/NeonDojo/open-sage-venv/bin/python" \
  tools/probe_open_sage.py --levels 1ply 2ply --output /tmp/open-sage-all.json
```

### Verification of this implementation

Run on November 12, 2026 on the local Apple Silicon Mac:

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` succeeded. The complete test run needed the explicit local test server because some existing SQLx tests are not marked as `ignored`.
- 492 frontend tests succeeded; the production build and OpenAPI/TypeScript generation succeeded. Vite still reports its existing bundle and import splitting notices.
- 121 explicitly enabled PostgreSQL/GNUbg regression cases for settings, configuration, benchmarks, estimates, result readers and databases succeeded, plus four checkpoint/lease recovery tests.
- Six new native installation/job/benchmark/provider/ETA tests and four native play tests succeeded. The benchmarks check the complete 460 checker moves and eleven executed play moves, respectively. The play tests cover both colors, complete games, a cube drop, accepting and rejecting resignations, resuming, settings after the start, and crossed bot/hint engines.
- 13 affected browser cases for settings, benchmark and real Sage selection succeeded; the additional installation conflict with reload and eight resignation dialog cases were also checked successfully. Desktop screenshots were checked visually at all three reference sizes.
- 39 Python/theme tests run: 34 succeeded, five existing optional tests skipped. Optional and native tests of the whole project that are not enabled across the board remain marked as `ignored`.
- Provider lookup with 10,000 synthetic analysis jobs and one matching Sage result: `EXPLAIN (ANALYZE, BUFFERS)` uses `jobs_engine_position_ready`, six shared-hit blocks, 0.027 ms database execution time. This is a single warm, highly selective test query; it makes no claim about API or UI latency.

This original verification took place in the isolated integration worktree; the user data was not migrated in the process. The subsequent transfer to the main worktree retains the pause/resume features added there. The native Sage test plays both colors to completion and additionally checks pause/resume after a change to the global engine defaults: engine snapshots, board position, history, library identity and SGF checkpoint are preserved.

### Adoption and local activation on November 12, 2026

The integration was transferred to the main worktree `/Users/hendrik/Developer/bgtutor` at `ca6631f`. The changes for pause/resume and decision dialogs that existed by then are retained. Original files of both worktrees, the previous local plan and check logs are under `backups/open-sage-integration-20261112-064429/`. The integration worktree remains as an additional reference.

Before the restart, the PostgreSQL database was fully backed up as `bgtutor-before-open-sage.dump` (172,823,656 bytes, SHA256 in `database-sha256.txt`). The table of contents was checked with PostgreSQL 17 `pg_restore`; the older local `pg_restore` does not support this archive format. Migration `0041` is applied. A comparison of row counts and content digests of all 31 previous tables before and after activation shows only the expected changes to `_sqlx_migrations`, `app_settings` and `configuration_state`; `engine_installations` is new. For `benchmark_runs`, the two new, empty metadata columns were excluded from the comparison. All 62 games, five play sessions, and existing analyses, profiles and study progress remained unchanged. No test games or test jobs were created in the user data.

At the user's explicit request, the stored defaults for new games and analyses are **Open Sage, 2-ply**. The benchmark selection remains independent. Existing play sessions and jobs keep their previous settings; stored profiles were not overwritten. The verified Python environment is at `local-data/open-sage/venv/bin/python`.

The optimized backend build runs at `http://127.0.0.1:8787` and serves `web/dist`. Its permanent executable is `local-data/bin/bgtutor`, and its log is `local-data/bgtutor.log`. The central `Cargo.toml` explicitly disables stripping for development and release builds to avoid the Rust LINKEDIT problem under macOS 27. The former manual prefix `CARGO_PROFILE_RELEASE_STRIP=none` is therefore no longer needed; release optimization stays active.

Verified successfully again after merging:

- Rust formatting, Clippy with `-D warnings`, complete workspace suite: 187 tests succeeded, 333 optional tests ignored as usual.
- Ten explicitly enabled native Sage tests for play, installation, jobs and complete benchmarks, plus the existing GNUbg pause/resume test. The two complete Sage games additionally check pause/resume with the stored engine identity.
- 492 frontend tests, 43 affected browser tests, OpenAPI/TypeScript generation and the frontend production build. The browser databases were separate from the user data.
- 39 Python/theme tests: 34 succeeded, five optional tests skipped.
- Optimized backend build with the strip step disabled, an actual installation check through the running API, both engines available in the catalog, and the stored Sage defaults read back.

### Remeasurement of the three optimizations – November 12, 2026

At the time of the remeasurement, the implementation was uncommitted in the main worktree. Existing changes to Engine Matches and other files were preserved by a three-way merge; separate backups are under `backups/sage-optimization-20261112-073609` and `backups/sage-performance-merge-20261112-075315`. The running user instance was not replaced or restarted during the remeasurement. In particular, its database was not used for tests or benchmarks, and this work did not activate the independent migration 0042 there. The optimized release build of the merged main worktree was measured on port 8891 with `neondojo_e2e_sage_perf_20261112`.

Three analysis and three play measurements per engine, run sequentially in alternating order, without parallel builds or tests. Hardware: Apple M2 Pro, 10 logical CPUs, 16 GiB RAM, macOS 27.0.1. GNUbg 1.08.003 20261108 with one thread; Sage 2.0.20260907 with four analysis threads and two play threads, respectively. The depth was numerically 2-ply in each case under the counting used at the time; under today's [ply convention](reference/settings.md#ply-convention), GNUbg's native 2-ply is called 3-ply. This is not evidence of identical search strength. The engine/model snapshots and reference checksums are identical to those of the baseline measurement. The OS file caches were not flushed.

Analysis: ten games, 460 checker moves including forced moves, and cube analysis. Play: eleven fixed checker moves with dice already rolled. The times include the complete NeonDojo pipeline and, for Sage, one cold sidecar start per run; queue wait, installation test, database writes and UI animation are excluded. The play figure is the median of the three per-run means, not percentiles of individual user moves.

| Measurement | Engine | Before: median | After: median | After: min–max | Time reduction |
| --- | --- | ---: | ---: | ---: | ---: |
| Analysis total (s) | gnubg | 20.215 | 18.442 | 18.368–18.636 | 8.8% |
| Analysis total (s) | bgsage | 49.750 | 12.915 | 12.893–12.955 | 74.0% |
| Play move, mean (ms) | gnubg | 229.470 | 224.945 | 224.723–226.190 | 2.0% |
| Play move, mean (ms) | bgsage | 484.310 | 302.656 | 301.610–304.460 | 37.5% |

Sage thus needs 74.0% less analysis time (3.85× throughput) and 37.5% less time per reference move (factor of 1.60). In the current comparison, Sage is 30.0% below GNUbg for analysis and still 34.5% above it for play moves. The GNUbg control also became faster, by 8.8% and 2.0% respectively, so the entire before/after difference cannot be attributed solely to the implementation. Normalized to the respective GNUbg control, the Sage/GNUbg time ratio drops by about 71.5% for analysis and 36.3% for play. Only run times were measured, not differences in playing strength.

The three changes can be traced independently: a native identity check reads 42 instead of 67 files, or 16,854,849 instead of 31,035,709 bytes; hashes are valid only within that check. The 738 individual decisions of the reference match run in ten exclusive game blocks with complete initial and final checks. The eleven Sage reference moves start eleven instead of 22 GNUbg rules processes. A rules process must exit cleanly before the result is returned; a crash, error exit, stderr output, timeout, invalid or overlong responses and cancellation discard the action.

Checks of the main worktree: `cargo fmt --all --check`, Clippy for all targets without warnings, 199 passing workspace tests (338 optional tests ignored), 492 frontend tests, frontend and release build. Full Python run: 49 tests, of which five existing optional tests were skipped. In addition, four native Sage play tests passed after the merge, as did six Sage configuration/job/benchmark tests, the native adapter tests, and the exact comparison of all 460 checker and 278 cube decisions between the single-request path and the game blocks. New error, cancellation, completion and concurrency tests check the process and batch boundaries. No UI contracts or views were changed; no additional browser end-to-end run was performed.

The extended GNUbg play test suite had 34 passing and four failing tests. All four failures were reproduced at the unchanged base commit `d2e0291`: the tests change `play_sessions.gnubg_path`, while new sessions use the immutable engine snapshot; they therefore wrongly expect HTTP 503 instead of the HTTP 200 that actually succeeds. The tests were neither weakened nor rebuilt in this task. Details are in the archived logs.

Measurement data, complete individual values, source and binary hashes and check logs: `local-data/benchmarks/20261112-075923-2ply-optimized/` (`results.json`, `summary.json`, `comparison.json`, `source-manifest.json`). Baseline measurement: `local-data/benchmarks/20261112-065856-2ply-comparison/`. The separate benchmark backend was stopped after the run; the test database and measurement artifacts are kept for review. No changes were committed or pushed during the remeasurement.

#### Activation of the optimizations

The optimizations needed no new database migration. For the local activation, only the verified Open Sage commit was built; independent local changes and their migrations remained in the main worktree. Before `local-data/bin/bgtutor` was replaced, the previous binary and a complete PostgreSQL dump were backed up. Active jobs had to finish before the restart. Afterwards, API health, available engines, unchanged settings and the content digests of all user tables were checked. The existing Open Sage defaults for play and analysis remained in place. Activation records are outside Git under `backups/sage-performance-activation-*/`.

### Patched local fork and 1-ply – November 12, 2026

The local installation used a separately built, non-editable ARM64 wheel from `HendrikReh/bgsage`, commit `2e71beed9c24b5bbc7564c03374d6d588ff24119` (issue #6). Base: `139427d826dd67798226e7d23c003c90315306ba`. The package name and the upstream version `2.0.20260907` were kept; the commit, wheel hash and complete runtime fingerprint distinguished this build from the original PyPI wheel.

Installation: `local-data/open-sage-fork-2e71bee/venv/bin/python`. The `requirements.lock`, `wheels/`, `provenance.json` and `probe-1ply-2ply.json` stored alongside it document and exactly reproduce this local build. The Sage wheel has SHA256 `d836dad115c39d8fc88e49c76500618a1fe421d3dadfb6420736324cfada2597`; the build is intended for the verified macOS 27 ARM64 with Python 3.12. Installation and check command at the time:

```sh
python3.12 -m venv /path/to/new-venv
/path/to/new-venv/bin/python -m pip install --no-index --only-binary=:all: --require-hashes \
  --find-links local-data/open-sage-fork-2e71bee/wheels \
  -r local-data/open-sage-fork-2e71bee/requirements.lock
/path/to/new-venv/bin/python tools/probe_open_sage.py --levels 1ply 2ply --output /tmp/sage-fork-probe.json
```

Enabling 1-ply added an optional `one_ply_verified` record to the immutable installation/job snapshot. Old snapshots without this field stay 2-ply and keep their previous form when serialized. The installation check sets the record only after a real 1-ply evaluation with the thread budget respected and identical artifact identity. Each started worker still checks the requested profile and the actual budget; RECORD, model and native hashes and the checks at request and game-block boundaries are retained. A missing or failed 1-ply check leaves only 2-ply available.

Play, analysis, benchmarks and engine matches store the selected depth separately per job or player. Benchmark protocols distinguish 1-ply from 2-ply; existing 2-ply protocols remain unchanged. Migration `0043` only extended the two Sage depth constraints, without changing user settings or data rows. The defaults remained Open Sage with 2-ply.

The previous environment `local-data/open-sage/venv` was kept unchanged for old games and jobs. At the time, switching back was done through "Local Open Sage installation → Verify and save" with the old Python path; new 1-ply defaults first had to be set back to 2-ply. Check and backup artifacts are under `backups/sage-fork-activation-20261112/`.

### Reused cube strategy and shorter rules startup

As of November 12, 2026. The local fork `HendrikReh/bgsage` (commit `5b146c7d86fa965f2185e3871f3c56df030fd76f`) keeps the native cube strategy per analyzer. It is loaded from the verified weights on the first cube call and stays separate from the checker strategy. Bear-off processing and temporary search state stay bound to the respective call. Cubeful 1-ply converts the candidates serially. In the real checker path, NeonDojo checks that the build declared as serial starts no candidate pool. For earlier builds and for 2-ply, the observed pool limits continued to apply.

At the same time, the shorter rules startup for direct GNUbg binaries was introduced (see [Performance and integrity boundaries](#performance-and-integrity-boundaries)).

The measured wheel (SHA-256 `20695d1835a7af26fcfb8fe25b3ddb094d391157cb70b0b72b68aae6d7dff2dc`) is installed with the corresponding `requirements.lock` under `local-data/open-sage-fork-optimized-20695d1/venv/bin/python`. The runtime fingerprint distinguishes it from the previous fork even though the package version stays the same. The test environment and the raw comparison data are stored locally under `local-data/benchmarks/*-sage-cube-reuse/`.

### Post-Crawford cube – November 18, 2026

Up to commit `5b146c7`, Open Sage undervalued "no double" for the trailing player after Crawford. In 15 positions from engine duels (3-away or 4-away against 1-away, cube centered), "no double" at 2-ply was 0.058 to 0.184 below "double/take". GNUbg (`hint` and a rollout with 1,296 games) and Open Sage's own rollout evaluate both actions equally there. The recommendation was still "double", but the analysis recorded an error for "no double". The fork fixes this in PR #9: `cl2cf_match` evaluates a cube that the trailing player can still turn as doubled once.

The build from `340fc5346f1938d505f99f22101e059a30e9cfc6` is under `local-data/open-sage-fork-340fc53/` with `src/`, `wheels/`, `requirements.lock`, `provenance.json`, `probe-1ply-2ply.json` (passed) and `venv/`. The Sage wheel has SHA256 `a9a264d1436f613dfdbecc7ec1bb5c624f8f6589ed26e09eaaaddc306bd1c00d`; the runtime fingerprint in NeonDojo was `7c9a021a…`. Analyses that previously ran with `5b146c7` keep their stored values. They must be reanalyzed to obtain corrected post-Crawford cube values.

On November 18, 2026, 101 local matches were reanalyzed with Open Sage 2-ply. All 101 jobs completed. According to the engine snapshot, they ran with `7c9a021a…`; the run took about two minutes. 6,452 cube decisions from the previous analyses (`5b741451…`, build `5b146c7`) were compared, paired by match, game, decision and player:

| Games | Decisions | Different best action | Cube error old − new | Larger error |
| --- | ---: | ---: | ---: | ---: |
| Post-Crawford | 77 | 27 | 1.507 | 0 |
| Other | 6,375 | 0 | 0.026 | 16 |

Only in post-Crawford games did "no double" shift by more than 0.03, and there it did so in all 77 decisions. In the other games the deviations stayed small: for "no double" the median was 0.0003, and for "double/take" the maximum was 0.03. The best action did not change anywhere there. The cause of these residual deviations has not been investigated. Build `340fc53` differs from `5b146c7` not only by the fix. Overall, the cube error dropped from 4.302 to 2.769. The number of cube errors above 0.02 dropped from 68 to 50. For checker decisions, the total error stayed practically the same (14.419 versus 14.422). The previous analyses are still stored.

### Fork release 2.0.20261119 – November 19, 2026

From November 19, 2026, NeonDojo used the fork release [`v2.0.20261119`](https://github.com/HendrikReh/bgsage/releases/tag/v2.0.20261119) (commit `0dee4f321c0437dc142da2ae68d26f7da3038db1`, model still `stage11`). It was the first fork package with its own version number. Earlier fork builds still reported `2.0.20260907`. The installation is under `local-data/open-sage-fork-2.0.20261119/`. It contains `wheels/`, `requirements.lock`, `provenance.json`, `probe-1ply-2ply.json` (passed) and `venv/`. The official cp312 wheel from the GitHub release with SHA256 `c476a0b6edbc32021a0c7c4a7397ffcd2d869502c04bbf060a2a12b37baec4d9` was used. The digest was checked against the release API. The runtime fingerprint in NeonDojo was `9b3ffc5c…`.

At that time, `SUPPORTED_VERSIONS` and `SUPPORTED_SAGE_PACKAGE_VERSIONS` contained `2.0.20261119` and still `2.0.20260907`, so that stored job and duel snapshots and the `open-sage-fork-340fc53` installation kept working. The upgrade to NeonGammon 3.0.0 reduced both lists to `3.0.0`.

According to the release notes, the results change: at 1- and 2-ply, the cube lifetime in money contact play depends on depth and pip count. In addition, the cubeful 2-ply checker path now searches only the candidates that survive the move filter at full depth. Open Sage returns the remaining candidates with 1-ply values in the same list. NeonDojo does not mix depths. The sidecar therefore discards candidates below the requested depth, as it already did for the cubeless path. The played move is always evaluated at full depth through `force_boards`; if the engine returns it at a shallower depth, the request fails. Play hints and the evaluation of the player's own move need every legal move. For them, NeonDojo requests all legal moves (`all_legal`). For opening 31 at 2-ply, this took about 35 ms instead of 12 ms. New 2-ply analyses therefore show fewer candidates, 5 instead of 16 in the test case. 3- and 4-ply still evaluate all candidates at full depth. This behavior applies unchanged to NeonGammon 3.0.0.

Analyses and duels that ran with `7c9a021a…` keep their stored values. They only receive new values through a new analysis.
