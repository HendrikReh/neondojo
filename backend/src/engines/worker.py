"""Bundled bounded JSON sidecar. Engine sources and assets remain untouched.

Rust injects the diagnostic probe module as ``integrity`` before calling main;
its RECORD/asset validation is the single shared integrity implementation.
"""
import hashlib
import json
import math
import os
import resource
import sys
import time
from itertools import groupby

MAX_REQUEST = 262144
MAX_RESPONSE = 2097152
ADAPTER_VERSION = 1
TOP_TWO_GAP = 0.010


def process_cpu_us():
    # RUSAGE_SELF covers every thread of this worker, including the native pool.
    usage = resource.getrusage(resource.RUSAGE_SELF)
    return round((usage.ru_utime + usage.ru_stime) * 1e6)


def validate_profile(plies, threads):
    if type(plies) is not int or not 1 <= plies <= 4 or type(threads) is not int or threads < 2:
        raise ValueError('Unsupported NeonGammon depth or thread count')


def probabilities(values):
    # Native float32 accumulation can exceed exact probability endpoints by
    # ~7.45e-9 (WC2014 game 5 bearoff). Bound rounding below one float32 epsilon;
    # larger errors and inconsistent nesting still fail closed.
    epsilon = 1e-7
    if len(values) != 5 or not all(math.isfinite(x) and -epsilon <= x <= 1 + epsilon for x in values):
        raise ValueError('Invalid NeonGammon probabilities')
    w, wg, wb, lg, lb = [min(1.0, max(0.0, x)) for x in values]
    if not (wb <= wg <= w and lb <= lg <= 1 - w + 1e-12):
        raise ValueError('Inconsistent NeonGammon probabilities')
    return [w, wg, wb, 1 - w, lg, lb]


def board(value):
    if not isinstance(value, list) or len(value) != 26 or any(type(x) is not int or abs(x) > 15 for x in value):
        raise ValueError('Invalid Sage board')
    if value[0] < 0 or value[25] < 0:
        raise ValueError('Sage bars must be nonnegative')
    if value[25] + sum(max(0, x) for x in value[1:25]) > 15 or value[0] + sum(max(0, -x) for x in value[1:25]) > 15:
        raise ValueError('Too many checkers')
    return value


def cube_context(request):
    context = request['context']
    return {key: context[key] for key in ('cube_value', 'cube_owner', 'away1', 'away2', 'is_crawford', 'jacoby')} | {'beaver': False}


def checker_dice(request):
    dice = request['dice']
    if len(dice) != 2 or any(type(x) is not int or not 1 <= x <= 6 for x in dice):
        raise ValueError('Invalid dice')
    return dice


class Runtime:
    def __init__(self, profile, integrity):
        validate_profile(profile['plies'], profile['threads'])
        integrity['deny_network']()
        integrity['validate_platform'](sys.platform == 'darwin' and 'Darwin' or sys.platform,
                                       __import__('platform').machine(), sys.version_info)
        import neongammon
        import bgbot_cpp
        self.sage, self.native, self.integrity = neongammon, bgbot_cpp, integrity
        self.profile = profile
        self.cubeless = profile.get('cubeless', False)
        if type(self.cubeless) is not bool:
            raise ValueError('Invalid NeonGammon cubeless flag')
        self.batch_active = False
        self.legacy_deep = None
        self.policy_analyzers = {}
        self.opponent_policies = {}
        self.identity = self.check_identity()
        self.release = integrity['RELEASES'][self.identity['package_version']]
        if self.release['presets'] and not callable(getattr(self.sage, 'DifficultyPolicy', None)):
            raise ValueError(f"NeonGammon {self.identity['package_version']} did not export DifficultyPolicy")
        if not callable(getattr(self.sage.BgBotAnalyzer, 'cube_decision', None)):
            raise ValueError('NeonGammon did not export BgBotAnalyzer.cube_decision; install the locked wheels')
        expected = profile.get('artifact_fingerprint')
        if expected and expected != self.identity['artifact_fingerprint']:
            raise ValueError('NeonGammon assets changed since submission; restore the saved installation')
        # Cubeless money duels use Sage's cubeless checker path, which evaluates
        # serially and searches only its filtered top candidates at full depth.
        self.analyzer = neongammon.BgBotAnalyzer(eval_level=f"{profile['plies']}ply", cubeful=not self.cubeless,
                                             parallel_threads=profile['threads'])
        self.models = None
        if self.release['shared_models']:
            self.models = neongammon.BatchModels.from_analyzer(self.analyzer)
        self.policy_analyzers[profile['plies']] = self.analyzer
        # Exercise the real candidate path and observe its configured budgets.
        opening, observed = integrity['observe_thread_budgets'](
            neongammon, lambda: self.analyzer.checker_play(list(neongammon.STARTING_BOARD), 3, 1, beaver=False))
        serial = self.cubeless or profile['plies'] == 1 and getattr(neongammon.analyzer, 'SERIAL_CUBEFUL_1PLY', False) is True
        integrity['validate_thread_budget'](profile['threads'], observed, serial=serial)
        if not opening.moves:
            raise ValueError('NeonGammon capability check returned no candidates')
        for move in opening.moves:
            integrity['validate_result'](move)
        self.verify()

    def check_identity(self):
        value = self.integrity['identity'](self.sage, self.native)
        try:
            self.integrity['validate_versions'](value)
        except self.integrity['ProbeError']:
            raise ValueError('Unsupported NeonGammon/NumPy version; install the locked wheels') from None
        # Include the RECORD file itself, dependency version, and model selection.
        distribution = __import__('importlib.metadata', fromlist=['distribution']).distribution('neongammon')
        record = distribution.read_text('RECORD')
        if not record:
            raise ValueError('Missing NeonGammon wheel RECORD')
        manifest = {k: value[k] for k in ('identity_sha256', 'neongammon', 'numpy', 'production_model', 'strategy')}
        manifest['record_sha256'] = hashlib.sha256(record.encode()).hexdigest()
        return {'package_version': value['neongammon'],
                'artifact_fingerprint': hashlib.sha256(json.dumps(manifest, sort_keys=True).encode()).hexdigest()}

    def verify(self):
        """Complete check: hash every locked file again and record the metadata guarding it."""
        # Record the metadata first: a change made while hashing leaves the
        # record stale, so the next guarded check repeats the complete one.
        paths = self.integrity['locked_paths'](self.sage, self.native)
        snapshot = self.integrity['stat_snapshot'](paths)
        if self.check_identity() != self.identity:
            raise ValueError('NeonGammon installation changed during evaluation')
        self.guard = paths, snapshot

    def verify_unchanged(self):
        """Check installation metadata at request and batch boundaries.

        Unchanged metadata of every locked path, recorded at the last complete
        check, stands for that check; any difference runs the complete check
        immediately, which rejects a changed installation as before.
        """
        guard = getattr(self, 'guard', None)
        if guard is None or self.integrity['stat_snapshot'](guard[0]) != guard[1]:
            self.verify()

    def top_two_candidates(self, before, dice, moves, context):
        if self.release['shared_models']:
            return self.analyzer.top_two_promote(before, moves, max_gap=TOP_TWO_GAP, **context)
        if len(moves) < 2 or self.sage.board.is_race(before):
            return moves, '2-ply'
        if any(move.eval_level != '2-ply' for move in moves[:2]):
            raise ValueError('NeonGammon did not evaluate both leading candidates at 2-ply')
        if moves[0].equity - moves[1].equity > TOP_TWO_GAP:
            return moves, '2-ply'
        # These releases expose a full 3-ply checker search. Keep precisely
        # the original pair and its tie ordering after the deeper search.
        if self.legacy_deep is None:
            self.legacy_deep = self.sage.BgBotAnalyzer(eval_level='3ply', parallel_threads=self.profile['threads'])
        pair = [move.board for move in moves[:2]]
        deep = self.legacy_deep.checker_play(before, *dice, force_boards=pair, **context)
        scored = [move for move in deep.moves if move.board in pair]
        if len(scored) != 2 or any(move.eval_level != '3-ply' for move in scored):
            raise ValueError('NeonGammon omitted a promoted candidate')
        return sorted(scored, key=lambda move: (-move.equity, -move.cubeless_equity, move.board)), '3-ply'

    def opponent_policy(self, difficulty, seed):
        if not self.release['presets']:
            raise ValueError('NeonGammon difficulty presets require NeonGammon 3.1.0 or newer')
        if not self.release['shared_models']:
            # Earlier releases expose the deterministic RNG seed only at construction.
            return self.sage.DifficultyPolicy(difficulty, seed=seed, parallel_threads=self.profile['threads'])
        if difficulty not in self.opponent_policies:
            depth = 2 if difficulty == 'full_strength' else 1
            if depth not in self.policy_analyzers:
                self.policy_analyzers[depth] = self.sage.BgBotAnalyzer(
                    eval_level=f'{depth}ply', models=self.models, parallel_threads=self.profile['threads'])
            analyzer = self.policy_analyzers[depth]
            self.opponent_policies[difficulty] = self.sage.DifficultyPolicy(
                difficulty, analyzer=analyzer, cube_analyzer=analyzer, seed=seed, parallel_threads=self.profile['threads'])
        policy = self.opponent_policies[difficulty]
        policy.reseed(seed)
        return policy

    def checker_input(self, request):
        before = board(request['board'])
        dice = checker_dice(request)
        forced = request.get('force_board')
        all_legal = request.get('all_legal', False)
        top_two = request.get('top_two', False)
        if (type(all_legal) is not bool or type(top_two) is not bool
                or sum((all_legal, top_two, forced is not None)) > 1
                or top_two and (self.cubeless or self.profile['plies'] != 2)):
            raise ValueError('Invalid checker candidate scope')
        force_boards = None
        if forced is not None:
            forced = board(forced)
            if forced not in self.sage.possible_moves(before, *dice):
                raise ValueError('Forced played board is not legal')
            force_boards = [forced]
        elif all_legal:
            force_boards = [board(list(move)) for move in self.sage.possible_moves(before, *dice)]
        return dict(board=before, die1=dice[0], die2=dice[1], force_boards=force_boards, **cube_context(request))

    def checker_output(self, request, native):
        before, dice = request['board'], request['dice']
        forced = request.get('force_board')
        depth = f"{self.profile['plies']}-ply"
        candidates = native.moves
        if request.get('top_two'):
            candidates, depth = self.top_two_candidates(before, dice, candidates, cube_context(request))
        from neongammon.text_export import compute_move_notation
        moves = []
        for move in candidates:
            try:
                self.integrity['validate_result'](move, probabilities)
            except Exception as error:
                raise ValueError(f'Invalid checker evaluation: board={before}, dice={dice}; {error}') from error
            if move.eval_level != depth:
                if not request.get('all_legal') and move.board != forced:
                    continue
                raise ValueError('NeonGammon returned a forced candidate below the requested depth')
            moves.append({'board': board(move.board), 'notation': compute_move_notation(before, move.board, *dice),
                          'equity': move.equity, 'probabilities': probabilities(move.probs.to_list())})
        if not moves or forced is not None and not any(move['board'] == forced for move in moves):
            raise ValueError('NeonGammon omitted the played candidate')
        return moves

    def cube_output(self, result):
        self.integrity['validate_result'](result, probabilities)
        if result.eval_level != f"{self.profile['plies']}-ply":
            raise ValueError('NeonGammon returned a cube evaluation below the requested depth')
        return {'no_double': result.equity_nd, 'take': result.equity_dt, 'pass': result.equity_dp,
                'probabilities': probabilities(result.probs.to_list())}

    def evaluate_batch(self, request):
        checker = request['operation'] == 'checker_batch'
        rows = request['rows']
        if (not self.batch_active or not isinstance(rows, list) or not 1 <= len(rows) <= 64
                or any(row['operation'] != ('checker' if checker else 'cube') for row in rows)):
            raise ValueError('Invalid verified batch rows')
        if not checker and (self.cubeless or any(row.get('decision_only') for row in rows)):
            raise ValueError('Invalid cube evaluation batch')
        inputs = [self.checker_input(row) if checker else dict(board=board(row['board']), **cube_context(row))
                  for row in rows]
        # Released wheels use one thread per native batch row. Deep checker
        # searches need the whole profile budget for high-branching doubles.
        depth = self.profile['plies']
        scalar = self.analyzer.checker_play if checker else self.analyzer.cube_action
        can_batch = (self.release['checker_batch'] and not self.cubeless and depth < 3
                     if checker else self.release['shared_models'])
        # The checker API takes Jacoby once per call. Cube inputs carry it per row.
        groups = groupby(inputs, key=lambda row: row['jacoby']) if checker and can_batch else [(False, inputs)]
        native = []
        for jacoby, group in groups:
            group = list(group)
            if not can_batch or len(group) == 1:
                native.extend(scalar(**row) for row in group)
                continue
            if self.models is None:  # 3.1.2 has no from_analyzer constructor.
                self.models = self.sage.BatchModels()
            evaluate = self.sage.batch_checker_play if checker else self.sage.batch_cube_action_nply
            native.extend(evaluate(group, eval_level=f"{depth}ply", jacoby=jacoby, beaver=False,
                                   models=self.models, n_threads=self.profile['threads']))
        return [self.checker_output(row, result) if checker else self.cube_output(result)
                for row, result in zip(rows, native, strict=True)]

    def dispatch(self, request):
        operation = request['operation']
        if operation == 'begin_batch':
            if self.batch_active:
                raise ValueError('Integrity batch already active')
            self.verify_unchanged()
            self.batch_active = True
            return self.identity, None
        if operation == 'end_batch':
            if not self.batch_active:
                raise ValueError('No active integrity batch')
            self.verify_unchanged()
            self.batch_active = False
            return self.identity, None
        if self.batch_active and operation not in ('checker', 'cube', 'checker_batch', 'cube_batch_nply'):
            raise ValueError('Unsupported integrity batch operation')
        if not self.batch_active:
            self.verify_unchanged()
        # Engine work only: integrity checks before and after stay outside.
        cpu_before, wall_before = process_cpu_us(), time.perf_counter_ns()
        if operation in ('checker_batch', 'cube_batch_nply'):
            result = self.evaluate_batch(request)
        elif operation == 'identity':
            result = self.identity
        else:
            before = board(request['board'])
            kwargs = cube_context(request)
            if operation in ('opponent_checker', 'opponent_cube'):
                difficulty, seed = request['difficulty'], request['seed']
                # Older Full Strength sessions pin 2-ply for both decisions.
                allowed_plies = (2, 3) if difficulty == 'full_strength' else (1,)
                if (self.cubeless or self.profile['plies'] not in allowed_plies
                        or type(seed) is not int or not 0 <= seed < 2**64):
                    raise ValueError('Invalid opponent policy request')
                policy = self.opponent_policy(difficulty, seed)
                if operation == 'opponent_checker':
                    dice = checker_dice(request)
                    if self.profile['plies'] == 3:
                        # NeonDojo pins a deeper checker search for new Full Strength matches.
                        selected = self.analyzer.checker_play(before, *dice, **kwargs).moves[0].board
                    else:
                        selected = policy.select_checker_move(before, *dice, **kwargs)
                    result = board(selected)
                else:
                    decision = policy.cube_decision(before, **kwargs)
                    result = {'should_double': decision.should_double, 'should_take': decision.should_take}
            elif operation == 'checker':
                result = self.checker_output(request, self.analyzer.checker_play(**self.checker_input(request)))
            elif request['operation'] == 'resignation':
                level = request['level']
                if type(level) is not int or level not in (1, 2, 3):
                    raise ValueError('Invalid resignation level')
                dice = request['dice']
                if dice[0] and dice[1]:
                    continuation = self.analyzer.checker_play(before, *dice, **kwargs).moves[0].equity
                else:
                    # Only the no-double equity matters; skip cube_action's probability search.
                    continuation = self.analyzer.cube_decision(before, **kwargs).equity_nd
                # Independent terminal outcome for the offered single/gammon/
                # backgammon. Sage performs its own match-equity conversion.
                terminal = [0] * 26
                terminal[25 if level == 3 else 6] = 1 if level == 1 else 15
                offered = self.analyzer.post_move_analytics(terminal, **{key: value for key, value in kwargs.items() if key != 'beaver'})
                self.integrity['validate_result'](offered, probabilities)
                if not math.isfinite(continuation):
                    raise ValueError('Invalid resignation continuation equity')
                result = {'accept': offered.cubeful_equity <= continuation + 1e-9,
                          'continuation': continuation, 'offered': offered.cubeful_equity}
            elif request['operation'] == 'cube':
                if self.cubeless:
                    raise ValueError('A cubeless NeonGammon profile makes no cube decisions')
                decision_only = request.get('decision_only', False)
                if type(decision_only) is not bool:
                    raise ValueError('Invalid cube decision scope')
                depth = f"{self.profile['plies']}-ply"
                if decision_only:
                    # Same equities as cube_action, without its second, cubeless
                    # search for probabilities the caller does not use.
                    result = self.analyzer.cube_decision(before, **kwargs)
                    equities = (result.equity_nd, result.equity_dt, result.equity_dp, result.optimal_equity)
                    if not all(math.isfinite(value) for value in equities):
                        raise ValueError(f'Invalid cube decision: board={before}, context={kwargs}')
                    if result.eval_level != depth:
                        raise ValueError('NeonGammon returned a cube evaluation below the requested depth')
                    result = {'no_double': result.equity_nd, 'take': result.equity_dt, 'pass': result.equity_dp}
                else:
                    result = self.cube_output(self.analyzer.cube_action(before, **kwargs))
            else:
                raise ValueError('Unsupported sidecar operation')
        cost = {'cpu_us': process_cpu_us() - cpu_before,
                'wall_us': (time.perf_counter_ns() - wall_before) // 1000}
        if not self.batch_active:
            self.verify_unchanged()
        return result, cost


def main(integrity):
    # Isolate native stdout chatter from the JSON transport, including C printf.
    protocol = os.fdopen(os.dup(1), 'w', buffering=1)
    os.dup2(2, 1)
    runtime = None
    while True:
        line = sys.stdin.buffer.readline(MAX_REQUEST + 1)
        if not line:
            return
        request_id = None
        try:
            if len(line) > MAX_REQUEST or not line.endswith(b'\n'):
                raise ValueError('Sidecar request exceeded its bound')
            request = json.loads(line)
            request_id = request['id']
            if type(request_id) is not int or request_id < 1:
                raise ValueError('Invalid request ID')
            if runtime is None:
                if request['operation'] != 'initialize':
                    raise ValueError('Sidecar handshake required')
                runtime = Runtime(request['profile'], integrity)
                result, cost = runtime.identity, None
            else:
                result, cost = runtime.dispatch(request)
            envelope = {'id': request_id, 'result': result}
            if cost is not None:
                envelope['cost'] = cost
            response = json.dumps(envelope, allow_nan=False, separators=(',', ':'))
            if len(response.encode()) + 1 > MAX_RESPONSE:
                if request['operation'] in ('checker_batch', 'cube_batch_nply') and len(request['rows']) > 1:
                    response = json.dumps({'id': request_id, 'result': {'split': True}})
                else:
                    raise ValueError('Sidecar output exceeded its bound')
            protocol.write(response + '\n')
        except Exception as error:
            # Fail closed: Rust discards the process after every error.
            protocol.write(json.dumps({'id': request_id, 'error': str(error)[:2000]}, allow_nan=False) + '\n')
            return
