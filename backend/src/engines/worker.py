"""Bundled bounded JSON sidecar. Engine sources and assets remain untouched.

Rust injects the diagnostic probe module as ``integrity`` before calling main;
its RECORD/asset validation is the single shared integrity implementation.
"""
import hashlib
import json
import math
import os
import sys

MAX_REQUEST = 262144
MAX_RESPONSE = 2097152
ADAPTER_VERSION = 1


def validate_profile(plies, threads):
    if plies not in (1, 2) or threads not in (2, 4):
        raise ValueError('Only verified Open Sage 1/2-ply with 2 or 4 threads is supported')


def probabilities(values):
    # Native float32 accumulation can exceed exact probability endpoints by
    # ~7.45e-9 (WC2014 game 5 bearoff). Bound rounding below one float32 epsilon;
    # larger errors and inconsistent nesting still fail closed.
    epsilon = 1e-7
    if len(values) != 5 or not all(math.isfinite(x) and -epsilon <= x <= 1 + epsilon for x in values):
        raise ValueError('Invalid Open Sage probabilities')
    w, wg, wb, lg, lb = [min(1.0, max(0.0, x)) for x in values]
    if not (wb <= wg <= w and lb <= lg <= 1 - w + 1e-12):
        raise ValueError('Inconsistent Open Sage probabilities')
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
        import bgsage
        import bgbot_cpp
        self.sage, self.native, self.integrity = bgsage, bgbot_cpp, integrity
        self.profile = profile
        self.batch_active = False
        self.identity = self.check_identity()
        expected = profile.get('artifact_fingerprint')
        if expected and expected != self.identity['artifact_fingerprint']:
            raise ValueError('Open Sage assets changed since submission; restore the saved installation')
        self.analyzer = bgsage.BgBotAnalyzer(eval_level=f"{profile['plies']}ply", parallel_threads=profile['threads'])
        # Exercise the real candidate path and observe its configured pool bound.
        from concurrent.futures import ThreadPoolExecutor
        from unittest.mock import patch
        observed = []
        def pool(*args, **kwargs):
            observed.append(kwargs.get('max_workers', args[0] if args else None))
            return ThreadPoolExecutor(*args, **kwargs)
        with patch('bgsage.analyzer.ThreadPoolExecutor', side_effect=pool):
            opening = self.analyzer.checker_play(list(bgsage.STARTING_BOARD), 3, 1, beaver=False)
        integrity['validate_thread_budget'](profile['threads'], observed)
        if not opening.moves:
            raise ValueError('Open Sage capability check returned no candidates')
        for move in opening.moves:
            integrity['validate_result'](move)
        self.verify()

    def check_identity(self):
        value = self.integrity['identity'](self.sage, self.native)
        if value['bgsage'] != '2.0.20260907' or value['numpy'] != '2.5.4':
            raise ValueError('Unsupported Open Sage/NumPy version; install the locked wheels')
        # Include the RECORD file itself, dependency version, and model selection.
        distribution = __import__('importlib.metadata', fromlist=['distribution']).distribution('bgsage')
        record = distribution.read_text('RECORD')
        if not record:
            raise ValueError('Missing Open Sage wheel RECORD')
        manifest = {k: value[k] for k in ('identity_sha256', 'bgsage', 'numpy', 'production_model', 'strategy')}
        manifest['record_sha256'] = hashlib.sha256(record.encode()).hexdigest()
        return {'package_version': value['bgsage'],
                'artifact_fingerprint': hashlib.sha256(json.dumps(manifest, sort_keys=True).encode()).hexdigest()}

    def verify(self):
        if self.check_identity() != self.identity:
            raise ValueError('Open Sage installation changed during evaluation')

    def dispatch(self, request):
        operation = request['operation']
        if operation == 'begin_batch':
            if self.batch_active:
                raise ValueError('Integrity batch already active')
            self.verify()
            self.batch_active = True
            return self.identity
        if operation == 'end_batch':
            if not self.batch_active:
                raise ValueError('No active integrity batch')
            self.verify()
            self.batch_active = False
            return self.identity
        if self.batch_active and operation not in ('checker', 'cube'):
            raise ValueError('Unsupported integrity batch operation')
        if not self.batch_active:
            self.verify()
        if request['operation'] == 'identity':
            result = self.identity
        else:
            before = board(request['board'])
            context = request['context']
            # Only explicit, validated context is passed; never arbitrary kwargs.
            kwargs = {key: context[key] for key in ('cube_value', 'cube_owner', 'away1', 'away2', 'is_crawford', 'jacoby')}
            kwargs['beaver'] = False
            if request['operation'] == 'checker':
                dice = request['dice']
                if len(dice) != 2 or any(type(x) is not int or x < 1 or x > 6 for x in dice):
                    raise ValueError('Invalid dice')
                forced = request.get('force_board')
                if forced is not None:
                    forced = board(forced)
                    if forced not in self.sage.possible_moves(before, *dice):
                        raise ValueError('Forced played board is not legal')
                result = self.analyzer.checker_play(before, *dice, force_boards=[forced] if forced is not None else None, **kwargs)
                from bgsage.text_export import compute_move_notation
                moves = []
                for move in result.moves:
                    try:
                        self.integrity['validate_result'](move, probabilities)
                    except Exception as error:
                        raise ValueError(f'Invalid checker evaluation: probabilities={move.probs.to_list()}, board={before}, dice={dice}, context={context}; {error}') from error
                    if move.eval_level != f"{self.profile['plies']}-ply":
                        raise ValueError('Open Sage returned a candidate below the requested depth')
                    moves.append({'board': board(move.board), 'notation': compute_move_notation(before, move.board, *dice),
                                  'equity': move.equity, 'probabilities': probabilities(move.probs.to_list())})
                if not moves or forced is not None and not any(move['board'] == forced for move in moves):
                    raise ValueError('Open Sage omitted the played candidate')
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
                result = self.analyzer.cube_action(before, **kwargs)
                try:
                    self.integrity['validate_result'](result, probabilities)
                except Exception as error:
                    raise ValueError(f'Invalid cube evaluation: probabilities={result.probs.to_list()}, board={before}, context={context}; {error}') from error
                result = {'no_double': result.equity_nd, 'take': result.equity_dt, 'pass': result.equity_dp,
                          'probabilities': probabilities(result.probs.to_list())}
            else:
                raise ValueError('Unsupported sidecar operation')
        if not self.batch_active:
            self.verify()
        return result


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
                result = runtime.identity
            else:
                result = runtime.dispatch(request)
            response = json.dumps({'id': request_id, 'result': result}, allow_nan=False, separators=(',', ':'))
            if len(response.encode()) + 1 > MAX_RESPONSE:
                raise ValueError('Sidecar output exceeded its bound')
            protocol.write(response + '\n')
        except Exception as error:
            # Fail closed: Rust discards the process after every error.
            protocol.write(json.dumps({'id': request_id, 'error': str(error)[:2000]}, allow_nan=False) + '\n')
            return
