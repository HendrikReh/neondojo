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
from types import ModuleType

MAX_REQUEST = 262144
MAX_RESPONSE = 2097152
ADAPTER_VERSION = 1


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
        self.opponent_policies = {}
        self.identity = self.check_identity()
        expected = profile.get('artifact_fingerprint')
        if expected and expected != self.identity['artifact_fingerprint']:
            raise ValueError('NeonGammon assets changed since submission; restore the saved installation')
        # Cubeless money duels use Sage's cubeless checker path, which evaluates
        # serially and searches only its filtered top candidates at full depth.
        self.analyzer = neongammon.BgBotAnalyzer(eval_level=f"{profile['plies']}ply", cubeful=not self.cubeless,
                                             parallel_threads=profile['threads'])
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
        if self.check_identity() != self.identity:
            raise ValueError('NeonGammon installation changed during evaluation')

    def opponent_policy(self, difficulty, seed):
        module_name = 'neondojo_opponent_policy'
        module = sys.modules.get(module_name)
        if module is None:
            module = ModuleType(module_name)
            module.__package__ = 'neongammon'
            sys.modules[module_name] = module
            exec(compile(OPPONENT_POLICY_SOURCE, 'neongammon/difficulty.py', 'exec'), module.__dict__)
        policy = self.opponent_policies.get(difficulty)
        if policy is None:
            policy = module.DifficultyPolicy(difficulty, seed=seed, parallel_threads=self.profile['threads'])
            self.opponent_policies[difficulty] = policy
        else:
            # The bundled policy has one RNG. Re-seed it independently of dice and other sessions.
            policy._rng.seed(seed)
        return policy

    def dispatch(self, request):
        operation = request['operation']
        if operation == 'begin_batch':
            if self.batch_active:
                raise ValueError('Integrity batch already active')
            self.verify()
            self.batch_active = True
            return self.identity, None
        if operation == 'end_batch':
            if not self.batch_active:
                raise ValueError('No active integrity batch')
            self.verify()
            self.batch_active = False
            return self.identity, None
        if self.batch_active and operation not in ('checker', 'cube'):
            raise ValueError('Unsupported integrity batch operation')
        if not self.batch_active:
            self.verify()
        # Engine work only: integrity checks before and after stay outside.
        cpu_before, wall_before = process_cpu_us(), time.perf_counter_ns()
        if request['operation'] == 'identity':
            result = self.identity
        else:
            before = board(request['board'])
            context = request['context']
            # Only explicit, validated context is passed; never arbitrary kwargs.
            kwargs = {key: context[key] for key in ('cube_value', 'cube_owner', 'away1', 'away2', 'is_crawford', 'jacoby')}
            kwargs['beaver'] = False
            if operation in ('checker', 'opponent_checker'):
                dice = request['dice']
                if len(dice) != 2 or any(type(x) is not int or x < 1 or x > 6 for x in dice):
                    raise ValueError('Invalid dice')
            if operation in ('opponent_checker', 'opponent_cube'):
                difficulty, seed = request['difficulty'], request['seed']
                expected_plies = 2 if difficulty == 'full_strength' else 1
                if (self.cubeless or self.profile['plies'] != expected_plies
                        or type(seed) is not int or not 0 <= seed < 2**64):
                    raise ValueError('Invalid opponent policy request')
                policy = self.opponent_policy(difficulty, seed)
                if operation == 'opponent_checker':
                    result = board(policy.select_checker_move(before, *dice, **kwargs))
                else:
                    decision = policy.cube_decision(before, **kwargs)
                    result = {'should_double': decision.should_double, 'should_take': decision.should_take}
            elif operation == 'checker':
                forced = request.get('force_board')
                all_legal = request.get('all_legal', False)
                if type(all_legal) is not bool or all_legal and forced is not None:
                    raise ValueError('Invalid checker candidate scope')
                force_boards = None
                if forced is not None:
                    forced = board(forced)
                    if forced not in self.sage.possible_moves(before, *dice):
                        raise ValueError('Forced played board is not legal')
                    force_boards = [forced]
                elif all_legal:
                    # Hints rate every legal move, so search all of them at full depth.
                    force_boards = [board(list(move)) for move in self.sage.possible_moves(before, *dice)]
                result = self.analyzer.checker_play(before, *dice, force_boards=force_boards, **kwargs)
                from neongammon.text_export import compute_move_notation
                moves = []
                for move in result.moves:
                    try:
                        self.integrity['validate_result'](move, probabilities)
                    except Exception as error:
                        raise ValueError(f'Invalid checker evaluation: probabilities={move.probs.to_list()}, board={before}, dice={dice}, context={context}; {error}') from error
                    if move.eval_level != f"{self.profile['plies']}-ply":
                        # The cubeless path and, from fork release 2.0.20261119,
                        # cubeful 2-ply prune weaker candidates to 1-ply. Keep
                        # only fully searched ones, which include the engine's
                        # choice; forced candidates must still reach full depth.
                        if not all_legal and move.board != forced:
                            continue
                        raise ValueError('NeonGammon returned a forced candidate below the requested depth')
                    moves.append({'board': board(move.board), 'notation': compute_move_notation(before, move.board, *dice),
                                  'equity': move.equity, 'probabilities': probabilities(move.probs.to_list())})
                if not moves or forced is not None and not any(move['board'] == forced for move in moves):
                    raise ValueError('NeonGammon omitted the played candidate')
                result = moves
            elif request['operation'] == 'resignation':
                level = request['level']
                if type(level) is not int or level not in (1, 2, 3):
                    raise ValueError('Invalid resignation level')
                dice = request['dice']
                if dice[0] and dice[1]:
                    continuation = self.analyzer.checker_play(before, *dice, **kwargs).moves[0].equity
                else:
                    continuation = self.analyzer.cube_action(before, **kwargs).equity_nd
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
                result = self.analyzer.cube_action(before, **kwargs)
                try:
                    self.integrity['validate_result'](result, probabilities)
                    if result.eval_level != f"{self.profile['plies']}-ply":
                        raise ValueError('NeonGammon returned a cube evaluation below the requested depth')
                except Exception as error:
                    raise ValueError(f'Invalid cube evaluation: probabilities={result.probs.to_list()}, board={before}, context={context}; {error}') from error
                result = {'no_double': result.equity_nd, 'take': result.equity_dt, 'pass': result.equity_dp,
                          'probabilities': probabilities(result.probs.to_list())}
            else:
                raise ValueError('Unsupported sidecar operation')
        cost = {'cpu_us': process_cpu_us() - cpu_before,
                'wall_us': (time.perf_counter_ns() - wall_before) // 1000}
        if not self.batch_active:
            self.verify()
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
                raise ValueError('Sidecar output exceeded its bound')
            protocol.write(response + '\n')
        except Exception as error:
            # Fail closed: Rust discards the process after every error.
            protocol.write(json.dumps({'id': request_id, 'error': str(error)[:2000]}, allow_nan=False) + '\n')
            return
