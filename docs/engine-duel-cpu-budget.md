# CPU budget for engine duels — concept

Status: step 1 (measurement, option A) is implemented; options B–D are proposals. Prompted by feedback on r/backgammon (November 2026): comparing engines "ply vs. ply" is unfair because plies are not comparable across engines. A fair duel should instead give both engines the same computing budget per decision.

## Problem

Engine duels currently pair `Competitor { engine_id, plies }` profiles ([engine_matches.rs](../backend/src/engine_matches.rs)). That compares labels, not effort:

- Ply counting differed. GNUbg 0-ply is a static evaluation, while XG calls the same thing 1-ply. Since migration 0079, Neondojo uses the XG convention for both engines and converts only when it talks to GNUbg ([Ply convention](reference/settings.md#ply-convention)). Duels from before 0079 ran a GNUbg side labelled N-ply at today's N+1-ply. Open Sage only offers 1–4-ply.
- The work per ply differs. Move filters and pruning decide how many candidates are searched deeper. GNUbg duels currently use GNUbg's default move filter, while analysis uses Top14. Open Sage passes no filter.
- Engines can be tuned differently, for example deeper for cube decisions and races than for normal checker play, with or without bear-off databases. A single depth setting for both sides cannot express that.
- Thread counts are global per engine. GNUbg defaults to 1, Open Sage uses at least 2 in duels.

As a result, "GNUbg 3-ply vs. Open Sage 3-ply" says little about which engine is stronger for the same effort.

## Facts about today's implementation

| Aspect | GNUbg | Open Sage |
|---|---|---|
| How a decision is made | Inside a GNUbg process via the native adapter: `set player N gnubg; nextturn` ([native.py](../backend/src/play/native.py)) | Persistent Python worker, `BgBotAnalyzer.checker_play` / `cube_action` ([worker.py](../backend/src/engines/worker.py)) |
| Process lifetime | New process per native step; with SGF replay. Against Sage: one process per Sage decision | Worker reused while the profile is unchanged. A Sage-vs-Sage duel at different depths restarts it on every switch |
| Settings | plies 0–4, cubeful, deterministic, noise 0, default move filter | `eval_level`, `parallel_threads`; `filter_max_moves` and `filter_threshold` are supported by the library but not exposed |
| Time limit | None. Only the 180 s timeout per call, which aborts | None. Only the 180 s timeout per call, which aborts and terminates the worker |
| Timing measurement in duels | None | None |

Neither engine offers a native time limit or an anytime search. A hard CPU limit per decision can therefore not simply be set. The process structure also adds a lot of overhead (process start, SGF replay, version and integrity checks) that has nothing to do with playing strength. Any measurement has to exclude it.

## Unit of the budget

**Recommendation: CPU time per turn, measured inside the engine process.**

- **CPU time instead of wall time.** It counts the work of all threads, as in "CPU cycles". It is largely unaffected by other load such as analysis jobs, and it makes engines with different thread counts comparable. Wall time is recorded alongside it because it is what the user waits for.
- **Per turn instead of per individual decision.** One GNUbg `nextturn` covers the cube decision and the checker play of a turn and cannot be split cleanly. For Open Sage, the turn consists of the `cube_action` call (if a double is possible) plus `checker_play`. A take/pass response counts as its own turn of the responding side.
- **Measuring point:** `resource.getrusage(RUSAGE_SELF)` (user + system time) and `time.perf_counter()`, both directly around `gnubg.nextturn()` in `native.py` and around the analyzer calls in `worker.py`. Process start, SGF loading, replay, `verify()` and version checks stay outside.
- **Implemented denominator:** all measured work of a side, including cube decisions, divided by its checker moves, including forced moves and passes. Turns without a choice cost almost nothing, and because paired dice give both sides similar forced turns, the step 1 implementation keeps them in the count. Excluding them would need a legal-move count for every GNUbg turn and remains possible later.

Caveat: the GNUbg measurement in the adapter also includes GNUbg's own move execution in `nextturn`. That is small compared with the evaluation, but it is not zero. The documentation must state this.

## Options

### A. Measure and report (foundation)

Each duel records the actual CPU and wall time per side. The series result shows Ø CPU ms per turn, p95 and the turns counted, next to the win rate.

- Answers the question "how much stronger, and at what cost?" without changing engine behaviour.
- Results stay deterministic and reproducible, because timing has no influence on play.
- Prerequisite for B and C.

### B. Budget-matched profiles (recommended next step)

The user sets a budget, for example "≤ 150 ms CPU per turn". Neondojo picks, for each engine, the strongest configuration whose measured average cost stays within the budget:

1. **Calibration:** for every candidate configuration (engine × plies × move filter), measure the cost on a fixed reference set of turns covering checker plays, cube decisions, contact positions, races and bear-off. The configuration, machine, engine version and thread count are recorded together.
2. **Selection:** for each side, the most expensive configuration that stays within the budget. If the two sides' actual costs differ by more than a tolerance (for example ±15 %), the result shows that clearly.
3. **Fine control:** plies alone are too coarse, since each ply is roughly an order of magnitude more expensive. That is exactly the criticism of "ply vs. ply". Move filters (GNUbg `movefilter`, Open Sage `filter_max_moves`/`filter_threshold`) provide the intermediate steps, which is why they need to be exposed in the duel profile.

Properties: the budget holds on average, not as a hard cap per turn. Play stays deterministic and the paired-dice comparison stays intact. The results are only valid for the measured machine. They are stored together with the machine record from the benchmark ([machine.rs](../backend/src/benchmarks/machine.rs)).

### C. Hard limit per turn (not now)

A deadline per turn. The engine returns the deepest result it finished in time.

- With today's engines, this is only possible as a depth ladder (1-ply, 2-ply, … each as a separate call, abort at the deadline). That wastes computation, and for GNUbg it would require restructuring: GNUbg would have to be asked per decision like an external engine instead of playing via `nextturn`.
- Timing would decide moves. Paired comparisons and exact replays would lose their reproducibility, because the same dice could lead to different plays depending on machine load.
- Worthwhile only when engines support a time limit themselves. This leads to point D.

### D. Engine profiles and external engines (later)

Engines like the "Quick" configuration announced on Reddit use different depths depending on the type of decision (cube, race). Instead of `plies`, a duel profile should be a named engine configuration: a base depth plus overrides for the cube and race, a move filter, and bear-off database yes/no. The budget then applies to that configuration as a whole.

For third-party engines, a small protocol would be needed (position, dice, match context, optional `budget_ms` → decision, depth used, CPU time used). Engines with their own time control could then also take part in a fair contest under C.

## Proposed order

1. **A: measurement (done).** `native.py` and `worker.py` measure CPU and wall time, and the runner stores sums per match and side, plus checker moves, in `engine_match_results` (migration 0075). The series statistics show CPU and elapsed time per move. Still open: maximum and p95 per turn, which would need per-turn values or a histogram.
2. **Expose move filters in the duel profile.** For GNUbg, the filter set explicitly (default vs. Top14, with the filter part of the snapshot); for Open Sage, `filter_max_moves`/`filter_threshold`. Recording the filter is a correctness gain even without a budget, because the current default is implicit.
3. **B: calibration and budget mode.** Reference set of turns, a calibration job (a persisted background job like the benchmark, with exclusive engine access), budget selection when a series is created, and display of planned vs. actual cost.
4. Decide on C/D only once an external engine with its own time control is available.

## Open questions

- **Budget unit:** CPU time (recommended) or wall time with a fixed, equal thread count?
- **Thread counts:** Should budget duels force equal thread counts? Open Sage requires at least 2, while GNUbg in duels mostly runs single-threaded.
- **Tolerance:** How large may the difference between the two sides' actual costs be before a result counts as "not budget-matched"?
- **Reference set:** Use the WC2014 benchmark match (460 checker plays, but analysis protocol) or a dedicated, phase-balanced set of turns including cube decisions?
- **Strength measurement:** Once the budget is the common axis, a cost/strength curve (win rate at several budgets) is more informative than a single duel. Do we want it as a series type?

## Cubeless money blocks

Implemented as duel format `money_cubeless`, following the suggestion that cubeless money games give a variance that is easy to state: about 1.35 points per game, so a 95% interval of roughly ±2.7/√N points per game. Checker play is compared without cube skill, which suits budget comparisons. The [Extras reference](reference/extras.md#engine-duels) describes the format; measured CPU time per move applies to it unchanged. Note that Open Sage's cubeless path runs serially with a narrower full-depth search, so its CPU time per move is far lower than in cubeful match play at the same nominal depth.

