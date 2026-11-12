#!/usr/bin/env python3
"""Offline, fail-closed compatibility probe for the locked Open Sage ARM64 build.

This is a diagnostic tool, not an application engine adapter. It never installs
packages, downloads assets, alters models, or substitutes evaluation results.
"""
import argparse
import base64
from datetime import datetime, timezone
import hashlib
import importlib.metadata
import inspect
import json
import math
from pathlib import Path
import platform
import resource
import subprocess
import sys
import time

VERSION = '2.0.20260907'


class ProbeError(Exception):
    """A failed compatibility requirement, safe to include in a report."""


def require(condition, message):
    if not condition:
        raise ProbeError(message)


def validate_platform(system, machine, version):
    require(system == 'Darwin' and machine == 'arm64' and tuple(version[:2]) == (3, 12),
            'Use native macOS ARM64 with Python 3.12 and the locked wheels (no Rosetta).')


def hash_required_file(path, label):
    require(path.is_file() and path.stat().st_size > 0,
            f'Missing or empty asset {label}. Reinstall from the locked wheels.')
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def validate_record_digest(actual_hex, expected_base64, label):
    actual_base64 = base64.urlsafe_b64encode(bytes.fromhex(actual_hex)).decode().rstrip('=')
    require(actual_base64 == expected_base64,
            f'Installed wheel integrity mismatch for {label}. Reinstall from the locked wheels.')


def validate_thread_budget(requested, observed, *, serial=False):
    if serial:
        require(not observed, f'Declared serial checker path created pools: {observed}')
        return
    require(observed and all(type(workers) is int and 1 <= workers <= requested for workers in observed),
            f'Candidate thread budget ignored: requested {requested}, observed pool limits {observed}. '
            'Do not enable this profile without an explicit bounded integration solution.')


def validate_probabilities(values):
    require(len(values) == 5 and all(math.isfinite(x) and 0 <= x <= 1 for x in values),
            'Non-finite or out-of-range probabilities')
    win, gw, bw, gl, bl = values
    require(bw <= gw <= win and bl <= gl <= 1 - win + 1e-12,
            'Inconsistent nested probabilities')


def validate_result(result, probability_validator=validate_probabilities):
    probability_validator(result.probs.to_list())
    for name in ('equity', 'cubeless_equity', 'cubeful_equity', 'equity_nd',
                 'equity_dt', 'equity_dp', 'optimal_equity', 'equity_diff'):
        if hasattr(result, name):
            require(math.isfinite(getattr(result, name)), f'Non-finite {name}')


def deny_network():
    """Deny Python socket/DNS use before imports; not a native syscall sandbox."""
    def audit(event, args):
        if event in ('socket.connect', 'socket.connect_ex', 'socket.getaddrinfo',
                     'socket.gethostbyname', 'socket.sendto', 'socket.bind'):
            raise ProbeError('Network access forbidden by offline probe')
    sys.addaudithook(audit)


def identity(sage, native):
    # Model slots and bearoff can refer to assets already in the manifest.
    # Reuse bytes only during this complete check, never across requests.
    digests = {}
    def checked_hash(path, label):
        key = path.resolve()
        if key not in digests:
            digests[key] = hash_required_file(path, label)
        return digests[key]

    package = Path(sage.__file__).parent
    files = {str(p.relative_to(package)): checked_hash(p, str(p.relative_to(package)))
             for p in sorted(package.rglob('*'))
             if p.is_file() and (p.suffix == '.py' or '_assets' in p.parts)}
    require(any(name.startswith('_assets/models/') for name in files),
            'Missing model assets. Reinstall from the locked wheels.')
    weights = sage.default_weights()
    for path in weights.weight_paths_list:
        checked_hash(Path(path), Path(path).name)
    from bgsage.weights import bearoff_db_path
    bearoff = bearoff_db_path()
    require(bearoff is not None, 'Missing bearoff model. Reinstall from the locked wheels.')
    checked_hash(Path(bearoff), 'bearoff database')
    native_path = Path(native.__file__)
    files[native_path.name] = checked_hash(native_path, native_path.name)
    records = {str(file): file.hash for file in importlib.metadata.distribution('bgsage').files or []}
    for label, file_digest in files.items():
        record = records.get(label if label == native_path.name else 'bgsage/' + label)
        require(record is not None and record.mode == 'sha256',
                f'Missing SHA256 wheel RECORD entry for {label}. Reinstall from the locked wheels.')
        validate_record_digest(file_digest, record.value, label)
    digest = hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()
    return {'bgsage': importlib.metadata.version('bgsage'),
            'numpy': importlib.metadata.version('numpy'), 'production_model': sage.PRODUCTION_MODEL,
            'strategy': weights.strategy_type, 'weight_slots': len(weights.weight_paths_list),
            'wheel_record_verified': True,
            'files_sha256': files, 'identity_sha256': digest}


def run_worker(skip_platform=False, levels=None):
    levels = levels or ['1ply', '2ply']
    started = time.perf_counter()
    report = {'schema_version': 1, 'status': 'failed', 'checks': [],
              'requested_levels': levels, 'primary_functional_level': levels[0],
              'excluded_profiles': {'3ply': 'Not validated or benchmarked', '4ply': 'Not validated or benchmarked',
                                    **({'1ply': 'Not requested in this probe run'} if '1ply' not in levels else {})},
              'platform': {'system': platform.system(), 'architecture': platform.machine(),
                           'python': platform.python_version(), 'macos': platform.mac_ver()[0]},
              'offline': {'python_network_audit': True,
                          'scope': 'Python sockets and DNS denied before engine imports; native networking is not intercepted'},
              'measured_at_utc': datetime.now(timezone.utc).isoformat()}
    deny_network()
    try:
        if not skip_platform:
            validate_platform(platform.system(), platform.machine(), sys.version_info)
        import_start = time.perf_counter()
        try:
            import bgsage as sage
            import bgbot_cpp as native
        except (ImportError, OSError) as error:
            raise ProbeError('Open Sage dependency/native library unavailable. Install the locked wheels into native Python 3.12; import failure: ' + type(error).__name__) from None
        report['import_seconds'] = time.perf_counter() - import_start
        report['identity'] = identity(sage, native)
        require(report['identity']['bgsage'] == VERSION, 'Unsupported bgsage version; install the locked wheels.')
        require(report['identity']['numpy'] == '2.5.4', 'Unsupported numpy version; install the locked wheels.')
        for name in ('checker_play', 'cube_action', 'post_move_analytics'):
            require(callable(getattr(sage.BgBotAnalyzer, name, None)), f'Missing API {name}')
        require('force_boards' in inspect.signature(sage.BgBotAnalyzer.checker_play).parameters,
                'Missing force_boards capability; this build cannot evaluate filtered played moves.')
        report['capabilities'] = ['checker_play', 'cube_action', 'post_move_analytics', 'force_boards']

        def check(name, function):
            start = time.perf_counter()
            try:
                detail = function()
                report['checks'].append({'name': name, 'status': 'passed',
                                         'seconds': time.perf_counter() - start, 'detail': detail})
            except Exception as error:
                report['checks'].append({'name': name, 'status': 'failed',
                                         'seconds': time.perf_counter() - start,
                                         'error': str(error) if isinstance(error, ProbeError) else type(error).__name__})

        init_start = time.perf_counter()
        analyzer = sage.BgBotAnalyzer(eval_level=levels[0], parallel_threads=2)
        report['first_analyzer_init_seconds'] = time.perf_counter() - init_start
        opening = list(sage.STARTING_BOARD)
        five_point = opening.copy()
        five_point[8] -= 1
        five_point[6] -= 1
        five_point[5] += 2

        def opening_case():
            legal = sage.possible_moves(opening, 3, 1)
            require(five_point in legal, 'Opening 8/5 6/5 missing from legal moves')
            result = analyzer.checker_play(opening, 3, 1, jacoby=True, beaver=False)
            require(result.moves and result.moves[0].board == five_point, 'Opening 31 does not choose 8/5 6/5')
            require({tuple(move.board) for move in result.moves} == {tuple(board) for board in legal}, 'Checker result does not cover legal candidates')
            for move in result.moves:
                validate_result(move)
            return {'best_board': five_point, 'candidates': len(result.moves),
                    'best_equity': result.moves[0].equity, 'level': result.eval_level}
        check('opening_31_five_point', opening_case)

        def notation_case():
            from bgsage.text_export import compute_move_notation
            require(compute_move_notation(opening, five_point, 3, 1) == '8/5 6/5', 'Opening notation mismatch')
            before = [0] * 26
            before[25], before[6], before[24], before[23] = 1, 14, -1, -14
            after = before.copy()
            after[25], after[0], after[24], after[22] = 0, 1, 0, 1
            require(after in sage.possible_moves(before, 1, 2), 'Independent bar-hit reference is not generated as legal')
            notation = compute_move_notation(before, after, 1, 2)
            require(notation == 'bar/24* 24/22', 'Bar-hit notation mismatch: ' + notation)
            blocked = before.copy()
            blocked[24], blocked[23] = -2, -13
            require(sage.possible_moves(blocked, 1, 2) == [blocked], 'Blocked notation fixture not a forced pass')
            require(compute_move_notation(blocked, blocked, 1, 2) == '', 'Pass notation must be empty')
            bearoff = [0] * 26
            bearoff[1], bearoff[24] = 2, -2
            borne_off = bearoff.copy()
            borne_off[1] = 0
            require(compute_move_notation(bearoff, borne_off, 1, 1) == '1/off(2)', 'Bearoff notation mismatch')
            return {'opening': '8/5 6/5', 'bar_hit': notation, 'pass': '', 'bearoff': '1/off(2)'}
        check('public_move_notation', notation_case)

        def perspective_case():
            board = [0] * 26
            board[0], board[25], board[6], board[24] = 2, 1, 14, -13
            expected = [0] * 26
            expected[0], expected[25], expected[19], expected[1] = 1, 2, -14, 13
            require(sage.flip_board(board) == expected, 'Asymmetric points/bars flip differs from independent reference')
            require(sage.flip_board(expected) == board, 'Perspective flip is not involutive')
            for current in (board, expected):
                result = analyzer.checker_play(current, 2, 1, beaver=False)
                require(result.moves, 'Bar entry incorrectly produces no moves')
                for move in result.moves:
                    validate_result(move)
                    require(move.board[25] < current[25], 'Bar entry did not use own bar')
            return {'own_bar_index': 25, 'opponent_bar_index': 0, 'both_bars_nonnegative': True}
        check('both_perspectives_and_bars', perspective_case)

        def money_case():
            values = []
            for jacoby in (False, True):
                for owner in ('centered', 'player', 'opponent'):
                    result = analyzer.cube_action(five_point, cube_value=1 if owner == 'centered' else 2,
                                                  cube_owner=owner, jacoby=jacoby, beaver=False)
                    validate_result(result)
                    require(not result.is_beaver, 'Beaver returned although disabled')
                    require(abs(result.equity_dp - 1) < 1e-8, 'Money pass equity is not +1 current cube')
                    values.append({'jacoby': jacoby, 'owner': owner, 'no_double': result.equity_nd,
                                   'double_take': result.equity_dt, 'double_pass': result.equity_dp})
            return values
        check('money_jacoby_cube_owners_beaver_disabled', money_case)

        def match_case():
            values = []
            for away1, away2, crawford in ((5, 3, False), (1, 3, True), (3, 1, True), (1, 3, False)):
                context = dict(away1=away1, away2=away2, is_crawford=crawford, beaver=False)
                result = analyzer.cube_action(five_point, **context)
                validate_result(result)
                if crawford:
                    require(not sage.can_double_match(away1, away2, 1, 'centered', True), 'Crawford permits doubling')
                    require(not result.should_double, 'Crawford cube evaluation recommends double')
                played = analyzer.checker_play(opening, 3, 1, **context)
                for move in played.moves:
                    validate_result(move)
                values.append({'away1': away1, 'away2': away2, 'crawford': crawford,
                               'no_double': result.equity_nd, 'double_take': result.equity_dt,
                               'double_pass': result.equity_dp, 'action': result.optimal_action})
            return values
        check('match_and_crawford', match_case)

        def pass_case():
            board = [0] * 26
            board[25], board[6] = 1, 14
            for point in range(19, 25):
                board[point] = -2
            board[18] = -3
            require(sage.possible_moves(board, 3, 1) == [board], 'Blocked bar must return only the unchanged pass board')
            moves = analyzer.checker_play(board, 3, 1, beaver=False).moves
            require(len(moves) == 1 and moves[0].board == board, 'Blocked bar checker result must evaluate the unchanged pass board')
            validate_result(moves[0])
            result = analyzer.post_move_analytics(board, jacoby=True)
            validate_result(result)
            return {'pass_post_move_equity': result.cubeful_equity, 'probs': result.probs.to_list()}
        check('forced_pass_post_move', pass_case)

        def bearoff_case():
            board = [0] * 26
            board[1], board[24] = 2, -2
            result = analyzer.checker_play(board, 1, 1, beaver=False)
            require(result.moves, 'Bearoff produced no moves')
            winner = result.moves[0]
            require(sage.check_game_over(winner.board) == 1, 'Two last checkers did not bear off for a single win')
            validate_result(winner)
            post = analyzer.post_move_analytics(winner.board, jacoby=False)
            validate_result(post)
            require(post.probs.to_list() == [1, 0, 0, 0, 0], 'Terminal single win probabilities incorrect')
            require(abs(post.cubeful_equity - 1) < 1e-8, 'Terminal single win equity incorrect')
            return {'probs': post.probs.to_list(), 'equity': post.cubeful_equity}
        check('bearoff_terminal_and_post_move', bearoff_case)

        def equity_units_case():
            winning = [0] * 26
            winning[24] = -1
            values = []
            for context in ({'jacoby': False}, {'away1': 5, 'away2': 3},
                            {'away1': 1, 'away2': 3, 'is_crawford': True}):
                for board, equity, probs in ((winning, 1, [1, 0, 0, 0, 0]),
                                             (sage.flip_board(winning), -1, [0, 0, 0, 0, 0])):
                    result = analyzer.post_move_analytics(board, **context)
                    validate_result(result)
                    require(result.probs.to_list() == probs, 'Terminal match probabilities incorrect')
                    require(abs(result.cubeful_equity - equity) < 1e-8, 'Terminal match equity is not normalized +/-1')
                    values.append({'context': context, 'cubeful_equity': result.cubeful_equity,
                                   'probs': result.probs.to_list()})
            return values
        check('match_equity_units', equity_units_case)

        def cube_units_case():
            board = [0] * 26
            board[1], board[24] = 1, -1
            money = analyzer.cube_action(board, cube_value=2, cube_owner='player', jacoby=False, beaver=False)
            validate_result(money)
            require(all(abs(actual - expected) < 1e-5 for actual, expected in
                        ((money.equity_nd, 1), (money.equity_dt, 2), (money.equity_dp, 1))),
                    'Money cube values are not in pre-offer cube units')
            require(not money.should_take and -money.equity_dp > -money.equity_dt,
                    'Responder perspective must prefer drop (-1) over take (-2)')
            matches = []
            for cube in (1, 2):
                result = analyzer.cube_action(board, cube_value=cube, cube_owner='player',
                                               away1=1, away2=3, jacoby=False, beaver=False)
                validate_result(result)
                require(all(abs(value - 1) < 1e-5 for value in
                            (result.equity_nd, result.equity_dt, result.equity_dp)),
                        'Match-winning cube values must saturate at normalized +1')
                matches.append({'cube': cube, 'no_double': result.equity_nd,
                                'double_take': result.equity_dt, 'double_pass': result.equity_dp})
            return {'money': {'no_double': money.equity_nd, 'double_take': money.equity_dt,
                              'double_pass': money.equity_dp}, 'match': matches,
                    'response_perspective': 'Negate original offer equity_dt/equity_dp; do not flip board and recompute'}
        check('cube_equity_units_and_response_perspective', cube_units_case)

        def force_case():
            deep = sage.BgBotAnalyzer(eval_level='2ply', cubeful=False, parallel_threads=2, filter_max_moves=1, filter_threshold=.001)
            first = deep.checker_play(opening, 3, 1, beaver=False)
            pruned = [move for move in first.moves if move.eval_level != '2-ply']
            require(pruned, 'Fixture does not exercise a filtered-out played move')
            played = pruned[-1].board
            forced = deep.checker_play(opening, 3, 1, beaver=False, force_boards=[played])
            actual = next((move for move in forced.moves if move.board == played), None)
            require(actual is not None and actual.eval_level == '2-ply', 'Played move not evaluated at requested depth')
            validate_result(actual)
            cubeful = sage.BgBotAnalyzer(eval_level='2ply', parallel_threads=2, filter_max_moves=1, filter_threshold=.001)
            cf_result = cubeful.checker_play(opening, 3, 1, beaver=False, force_boards=[played])
            require(all(move.eval_level == '2-ply' for move in cf_result.moves), 'Cubeful candidate not evaluated at full depth')
            for move in cf_result.moves:
                validate_result(move)
            return {'cubeful_all_candidates_full_depth': True, 'played_board': played, 'before_level': pruned[-1].eval_level,
                    'after_level': actual.eval_level, 'equity': actual.equity}
        check('forced_played_move_2ply', force_case)

        if len(levels) > 1:
            for level in levels[1:]:
                analyzer = sage.BgBotAnalyzer(eval_level=level, parallel_threads=2)
                for name, case in (('opening_31_five_point', opening_case),
                                   ('both_perspectives_and_bars', perspective_case),
                                   ('money_jacoby_cube_owners_beaver_disabled', money_case),
                                   ('match_and_crawford', match_case),
                                   ('forced_pass_post_move', pass_case),
                                   ('bearoff_terminal_and_post_move', bearoff_case),
                                   ('match_equity_units', equity_units_case),
                                   ('cube_equity_units_and_response_perspective', cube_units_case)):
                    check(name + '_' + level, case)

        report['thread_budgets'] = []
        # Transparent constructor observation: delegates to the real executor;
        # neither alters pool sizes nor substitutes any engine computation.
        from concurrent.futures import ThreadPoolExecutor
        from unittest.mock import patch
        for threads in (2, 4):
            for level in levels:
                def thread_case(threads=threads, level=level):
                    observed = []
                    def observe_pool(*args, **kwargs):
                        observed.append(kwargs.get('max_workers', args[0] if args else None))
                        return ThreadPoolExecutor(*args, **kwargs)
                    engine = sage.BgBotAnalyzer(eval_level=level, parallel_threads=threads)
                    with patch('bgsage.analyzer.ThreadPoolExecutor', side_effect=observe_pool):
                        engine.checker_play(opening, 3, 1, beaver=False)
                    serial = level == '1ply' and getattr(sage.analyzer, 'SERIAL_CUBEFUL_1PLY', False) is True
                    detail = {'level': level, 'requested_threads': threads, 'serial_checker': serial,
                              'observed_candidate_pool_limits': observed,
                              'scope': 'Real Python ThreadPoolExecutor constructor, not simultaneous native thread census'}
                    report['thread_budgets'].append(detail)
                    validate_thread_budget(threads, observed, serial=serial)
                    return detail
                check(f'thread_budget_{level}_{threads}', thread_case)

        report['benchmarks'] = []
        for threads in (2, 4):
            for level in levels:
                start = time.perf_counter()
                bench = sage.BgBotAnalyzer(eval_level=level, parallel_threads=threads)
                init = time.perf_counter() - start
                bench.checker_play(opening, 3, 1, beaver=False)
                samples = []
                for _ in range(3):
                    start = time.perf_counter()
                    result = bench.checker_play(opening, 3, 1, beaver=False)
                    samples.append(time.perf_counter() - start)
                    for move in result.moves:
                        validate_result(move)
                report['benchmarks'].append({'requested_threads': threads, 'level': level,
                    'scope': 'opening 31 checker_play; one warmup, three sequential calls; default move filter',
                    'init_seconds': init, 'warm_seconds': samples})
        report['status'] = 'passed' if all(c['status'] == 'passed' for c in report['checks']) else 'failed'
    except Exception as error:
        report['error'] = str(error) if isinstance(error, ProbeError) else type(error).__name__
    report['worker_seconds'] = time.perf_counter() - started
    report['peak_rss_bytes'] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * (1 if sys.platform == 'darwin' else 1024)
    report['rss_scope'] = 'Whole worker process high-water RSS, imports, hashing, all sequential analyzers and cases; not per-request memory'
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--levels', nargs='+', choices=('1ply', '2ply'), default=['1ply', '2ply'],
                        help='Explicit compatibility scope; 1ply requires a build with bounded candidate threads')
    parser.add_argument('--timeout', type=float, default=300)
    parser.add_argument('--worker', action='store_true', help=argparse.SUPPRESS)
    parser.add_argument('--skip-platform-check', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.worker:
        report = run_worker(args.skip_platform_check, args.levels)
    else:
        start = time.perf_counter()
        try:
            process = subprocess.run([sys.executable, str(Path(__file__).resolve()), '--worker', '--levels', *args.levels],
                                     capture_output=True, text=True, timeout=args.timeout)
            # Native engine may print diagnostic lines; worker JSON is the last line.
            report = json.loads(process.stdout.splitlines()[-1])
            require(process.returncode == 0 or report['status'] == 'failed', 'Worker failed without a valid failure report')
        except subprocess.TimeoutExpired:
            report = {'schema_version': 1, 'status': 'failed', 'error': 'Probe timed out; worker terminated. Check native installation and configured timeout.'}
        except (ValueError, IndexError, ProbeError):
            report = {'schema_version': 1, 'status': 'failed', 'error': 'Native worker crashed or returned an invalid report. Reinstall the locked ARM64 wheels.'}
        report['cold_process_seconds'] = time.perf_counter() - start
        report['cold_scope'] = 'Fresh Python worker including imports, asset hashes, all checks and warm benchmarks; OS file caches not cleared'
    text = json.dumps(report, sort_keys=True, allow_nan=False, indent=None if args.worker else 2)
    if args.output:
        args.output.write_text(text + '\n')
    print(text)
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    sys.exit(main())
