"""Dependency-free tests for the bundled NeonGammon sidecar protocol."""
import ast
import importlib.util
import os
from pathlib import Path
import unittest
import subprocess
import sys
sys.dont_write_bytecode = True
import json
from types import SimpleNamespace
from unittest.mock import Mock, patch

PATH = Path(__file__).resolve().parents[1] / 'backend/src/engines/worker.py'
_PROBE_SPEC = importlib.util.spec_from_file_location('probe_open_sage', Path(__file__).with_name('probe_open_sage.py'))
PROBE = importlib.util.module_from_spec(_PROBE_SPEC)
_PROBE_SPEC.loader.exec_module(PROBE)

class WorkerTests(unittest.TestCase):
    def test_worker_exists(self):
        self.assertTrue(PATH.exists(), 'bundled sidecar is missing')

    def test_worker_uses_public_engine_interfaces(self):
        tree = ast.parse(PATH.read_text())
        self.assertEqual([n.attr for n in ast.walk(tree)
                          if isinstance(n, ast.Attribute) and n.attr.startswith('_')], [])
        self.assertEqual([n.args[1].value for n in ast.walk(tree) if isinstance(n, ast.Call)
                          and isinstance(n.func, ast.Name) and n.func.id == 'getattr'
                          and isinstance(n.args[1], ast.Constant) and n.args[1].value.startswith('_')], [])

    def test_validates_depth_and_minimum_threads(self):
        if not PATH.exists():
            self.fail('bundled sidecar is missing')
        spec = importlib.util.spec_from_file_location('sage_worker', PATH)
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        worker.validate_profile(1, 2)
        worker.validate_profile(1, 4)
        worker.validate_profile(2, 2)
        worker.validate_profile(2, 4)
        worker.validate_profile(3, 2)
        worker.validate_profile(4, 2)
        limits = json.loads((PATH.parents[3] / 'shared/configuration-options.json').read_text())
        minimum = limits['engine_threads']['bgsage']['min']
        worker.validate_profile(2, minimum)
        worker.validate_profile(2, 64)
        for plies, threads in [(0, 2), (5, 2), (2, minimum - 1), (2, 2.5)]:
            with self.subTest(plies=plies, threads=threads), self.assertRaises(ValueError):
                worker.validate_profile(plies, threads)

    def test_only_float32_endpoint_roundoff_is_normalized(self):
        spec = importlib.util.spec_from_file_location('sage_worker', PATH)
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        self.assertEqual(worker.probabilities([1.0000000074505806, 0, 0, -7.450580596923828e-9, -7.450580596923828e-9]), [1,0,0,0,0,0])
        for values in [[1.000001,0,0,0,0], [.5,.1,.2,0,0], [.5,0,0,-.000001,0]]:
            with self.assertRaises(ValueError):
                worker.probabilities(values)

    def test_probability_conversion_preserves_inclusive_gammons(self):
        if not PATH.exists():
            self.fail('bundled sidecar is missing')
        spec = importlib.util.spec_from_file_location('sage_worker', PATH)
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        self.assertEqual(worker.probabilities([.6, .2, .05, .1, .02]), [.6, .2, .05, .4, .1, .02])
        for values in [[float('nan'), 0, 0, 0, 0], [.3, .4, .1, 0, 0], [.6, 0, 0, .5, 0]]:
            with self.assertRaises(ValueError):
                worker.probabilities(values)

class ProtocolTests(unittest.TestCase):
    def run_protocol(self, data):
        source = PATH.read_text() + """
class Runtime:
    def __init__(self, profile, integrity):
        self.identity = {'artifact_fingerprint': 'test'}
        print('native diagnostic noise', flush=True)
    def dispatch(self, request):
        return request['value'], request.get('cost')
main({})
"""
        return subprocess.run([sys.executable, '-I', '-u', '-c', source], input=data,
                              capture_output=True, timeout=5)

    def test_stdout_noise_is_separated_and_ids_are_preserved(self):
        result = self.run_protocol(b'{"id":1,"operation":"initialize","profile":{}}\n{"id":2,"operation":"test","value":42}\n')
        responses = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual([response['id'] for response in responses], [1, 2])
        self.assertEqual(responses[1]['result'], 42)
        self.assertIn(b'native diagnostic noise', result.stderr)

    def test_engine_cost_travels_beside_the_result(self):
        result = self.run_protocol(b'{"id":1,"operation":"initialize","profile":{}}\n'
                                   b'{"id":2,"operation":"test","value":1,"cost":{"cpu_us":7,"wall_us":5}}\n'
                                   b'{"id":3,"operation":"test","value":2}\n')
        responses = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertNotIn('cost', responses[0])
        self.assertEqual(responses[1], {'id': 2, 'result': 1, 'cost': {'cpu_us': 7, 'wall_us': 5}})
        self.assertNotIn('cost', responses[2])

    def test_malformed_and_oversized_requests_terminate_worker(self):
        for payload in (b'not json\n', b'x' * 262145, b'{"id":-1}\n'):
            with self.subTest(payload_length=len(payload)):
                result = self.run_protocol(payload)
                self.assertIn('error', json.loads(result.stdout))

    def test_nonfinite_response_is_rejected(self):
        result = self.run_protocol(b'{"id":1,"operation":"initialize","profile":{}}\n{"id":2,"operation":"test","value":NaN}\n')
        response = json.loads(result.stdout.splitlines()[-1])
        self.assertIn('error', response)

class BatchTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location('sage_worker', PATH)
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        self.runtime = worker.Runtime.__new__(worker.Runtime)
        self.runtime.batch_active = False
        self.runtime.cubeless = False
        self.runtime.profile = {'plies': 2}
        self.runtime.identity = {'package_version': 'fixture', 'artifact_fingerprint': 'original'}
        self.runtime.check_identity = Mock(side_effect=lambda: dict(self.current_identity))
        self.current_identity = dict(self.runtime.identity)
        self.runtime.sage = self.runtime.native = None
        self.metadata = ('original',)
        self.runtime.integrity = {'validate_result': lambda result, validator: validator(result.probs.to_list()),
                                  'locked_paths': lambda sage, native: ['locked'],
                                  'stat_snapshot': lambda paths: self.metadata}
        self.runtime.analyzer = SimpleNamespace(cube_action=lambda *args, **kwargs: SimpleNamespace(
            eval_level='2-ply', equity_nd=.2, equity_dt=.3, equity_dp=1, probs=SimpleNamespace(to_list=lambda: [.6, .1, .02, .1, .01])))
        self.cube = {'operation': 'cube', 'board': [0] * 26,
                     'context': dict(cube_value=1, cube_owner='centered', away1=0, away2=0, is_crawford=False, jacoby=True)}

    def test_keeps_only_fully_searched_candidates_and_requires_full_depth_played_move(self):
        full = SimpleNamespace(eval_level='2-ply', board=[0] * 26, equity=.1,
                               probs=SimpleNamespace(to_list=lambda: [.6, .1, .02, .1, .01]))
        pruned = SimpleNamespace(eval_level='1-ply', board=[1] + [0] * 25, equity=.3,
                                 probs=SimpleNamespace(to_list=lambda: [.6, .1, .02, .1, .01]))
        self.runtime.analyzer.checker_play = lambda *args, **kwargs: SimpleNamespace(moves=[full, pruned])
        self.runtime.sage = SimpleNamespace(possible_moves=lambda *args: [[0] * 26, [1] + [0] * 25])
        checker = dict(self.cube, operation='checker', dice=[3, 1])
        with patch.dict(sys.modules, {'neongammon.text_export': SimpleNamespace(compute_move_notation=lambda *args: '8/5 6/5')}):
            for cubeless in (False, True):
                with self.subTest(cubeless=cubeless):
                    self.runtime.cubeless = cubeless
                    moves, _ = self.runtime.dispatch(checker)
                    self.assertEqual([move['board'] for move in moves], [[0] * 26])
            self.runtime.cubeless = False
            with self.assertRaisesRegex(ValueError, 'forced candidate below the requested depth'):
                self.runtime.dispatch(dict(checker, force_board=[1] + [0] * 25))
            moves, _ = self.runtime.dispatch(dict(checker, force_board=[0] * 26))
            self.assertEqual([move['board'] for move in moves], [[0] * 26])
            with self.assertRaisesRegex(ValueError, 'forced candidate below the requested depth'):
                self.runtime.dispatch(dict(checker, all_legal=True))
            for scope in ({'all_legal': 1}, {'all_legal': True, 'force_board': [0] * 26}):
                with self.assertRaisesRegex(ValueError, 'Invalid checker candidate scope'):
                    self.runtime.dispatch(dict(checker, **scope))
            self.runtime.cubeless = True
        with self.assertRaisesRegex(ValueError, 'no cube decisions'):
            self.runtime.dispatch(self.cube)

    def test_all_legal_scope_forces_every_legal_board(self):
        legal = [[0] * 26, [1] + [0] * 25]
        calls = []
        def checker_play(*args, force_boards=None, **kwargs):
            calls.append(force_boards)
            return SimpleNamespace(moves=[SimpleNamespace(eval_level='2-ply', board=list(b), equity=.1,
                                                          probs=SimpleNamespace(to_list=lambda: [.6, .1, .02, .1, .01]))
                                          for b in legal])
        self.runtime.analyzer.checker_play = checker_play
        self.runtime.sage = SimpleNamespace(possible_moves=lambda *args: [tuple(b) for b in legal])
        checker = dict(self.cube, operation='checker', dice=[3, 1], all_legal=True)
        with patch.dict(sys.modules, {'neongammon.text_export': SimpleNamespace(compute_move_notation=lambda *args: '8/5 6/5')}):
            moves, _ = self.runtime.dispatch(checker)
        self.assertEqual(calls, [legal])
        self.assertEqual([move['board'] for move in moves], legal)

    def test_top_two_only_changes_the_checker_request_and_keeps_cube_at_two_ply(self):
        boards = [[0, 0, 0, 0, 0, 0, n] + [0] * 19 for n in (1, 2, 3)]
        probs = SimpleNamespace(to_list=lambda: [.6, .1, .02, .1, .01])
        moves = [SimpleNamespace(eval_level='2-ply', board=b, equity=e, probs=probs)
                 for b, e in zip(boards, [.15, .145, .14])]
        self.runtime.analyzer.checker_play = Mock(return_value=SimpleNamespace(moves=moves))
        self.runtime.analyzer.cube_action = Mock(wraps=self.runtime.analyzer.cube_action)
        self.runtime.top_two_candidates = Mock(return_value=([
            SimpleNamespace(eval_level='3-ply', board=b, equity=e, probs=probs)
            for b, e in zip(boards[1::-1], [.16, .15])], '3-ply'))
        checker = dict(self.cube, operation='checker', dice=[3, 1])
        notation = SimpleNamespace(compute_move_notation=lambda *args: '8/5 6/5')
        with patch.dict(sys.modules, {'neongammon.text_export': notation}):
            standard, _ = self.runtime.dispatch(checker)
            selected, _ = self.runtime.dispatch(dict(checker, top_two=True))
            self.assertEqual([move['board'] for move in standard], boards)
            self.assertEqual([move['board'] for move in selected], boards[1::-1])
            self.runtime.top_two_candidates.assert_called_once()
            with self.assertRaisesRegex(ValueError, 'Invalid checker candidate scope'):
                self.runtime.dispatch(dict(checker, top_two=True, all_legal=True))
        cube, _ = self.runtime.dispatch(self.cube)
        self.assertEqual(cube['take'], .3)
        self.runtime.analyzer.cube_action.assert_called_once()

    def test_checker_batch_matches_scalar_with_per_row_force_and_jacoby(self):
        self.runtime.models = object()
        self.runtime.release = PROBE.RELEASES['3.1.2']
        self.runtime.profile['threads'] = 4
        before = self.cube['board']
        played = [0, 1] + [0] * 24
        moves = [SimpleNamespace(board=played, eval_level='2-ply', equity=.2,
                                 probs=SimpleNamespace(to_list=lambda: [.6, .1, .02, .1, .01]))]
        self.runtime.analyzer.checker_play = Mock(return_value=SimpleNamespace(moves=moves))
        batch = Mock(side_effect=lambda rows, **kwargs: [SimpleNamespace(moves=moves) for _ in rows])
        self.runtime.sage = SimpleNamespace(batch_checker_play=batch, possible_moves=lambda *args: [played])
        rows = [dict(self.cube, operation='checker', dice=[6, 2], force_board=played),
                dict(self.cube, operation='checker', dice=[1, 3], context=dict(self.cube['context'], jacoby=False))]
        rows = [rows[0], rows[0], rows[1], rows[1]]
        self.runtime.dispatch({'operation': 'begin_batch'})
        notation = SimpleNamespace(compute_move_notation=lambda *args: '8/2')
        with patch.dict(sys.modules, {'neongammon.text_export': notation}):
            expected = [self.runtime.dispatch(row)[0] for row in rows]
            result, _ = self.runtime.dispatch(dict(operation='checker_batch', rows=rows))
            self.assertEqual(result, expected)
            self.assertEqual(batch.call_count, 2)
            for call, row in zip(batch.call_args_list, rows[::2]):
                self.assertEqual(call.args[0], [dict(board=before, die1=row['dice'][0], die2=row['dice'][1],
                                                     force_boards=[played] if 'force_board' in row else None,
                                                     beaver=False, **row['context'])] * 2)
                self.assertEqual(call.kwargs, dict(jacoby=row['context']['jacoby'], beaver=False,
                                                   models=self.runtime.models, n_threads=4, eval_level='2ply'))
            # Cubeless profiles and pre-parity wheels use ordered scalar evaluations.
            for cubeless, version in [(True, '3.2.0'), (False, '3.1.1')]:
                self.runtime.cubeless = cubeless
                self.runtime.release = PROBE.RELEASES[version]
                self.assertEqual(self.runtime.dispatch(dict(operation='checker_batch', rows=rows))[0], expected)
                self.assertEqual(batch.call_count, 2)

    def test_cube_batch_requires_verified_full_evaluations_and_exact_row_count(self):
        self.runtime.models = object()
        self.runtime.release = PROBE.RELEASES['3.2.0']
        self.runtime.profile['threads'] = 2
        cube = self.runtime.analyzer.cube_action(None)
        batch = Mock(return_value=[cube, cube])
        self.runtime.sage = SimpleNamespace(batch_cube_action_nply=batch)
        request = dict(operation='cube_batch_nply', rows=[self.cube, self.cube])
        with self.assertRaisesRegex(ValueError, 'verified batch'):
            self.runtime.dispatch(request)
        self.runtime.dispatch({'operation': 'begin_batch'})
        result, _ = self.runtime.dispatch(request)
        self.assertEqual(result, [self.runtime.dispatch(self.cube)[0]] * 2)
        self.assertIs(batch.call_args.kwargs['models'], self.runtime.models)
        self.assertEqual(batch.call_args.kwargs['eval_level'], '2ply')
        batch.return_value = [cube]
        with self.assertRaises(ValueError):
            self.runtime.dispatch(request)

    def test_deep_checkers_and_singletons_use_configured_scalar_analyzer(self):
        self.runtime.models = object()
        self.runtime.release = PROBE.RELEASES['3.2.0']
        self.runtime.sage = SimpleNamespace(possible_moves=lambda *args: [self.cube['board']],
                                            batch_checker_play=Mock(), batch_cube_action_nply=Mock())
        self.runtime.dispatch({'operation': 'begin_batch'})
        checker = dict(self.cube, operation='checker', dice=[2, 2], force_board=self.cube['board'])
        for depth in (2, 3, 4):
            self.runtime.profile.update(plies=depth, threads=4)
            cube = self.runtime.analyzer.cube_action(None)
            cube.eval_level = f'{depth}-ply'
            self.runtime.analyzer.cube_action = Mock(return_value=cube)
            self.runtime.analyzer.checker_play = Mock(return_value=SimpleNamespace(moves=[
                SimpleNamespace(board=checker['board'], eval_level=cube.eval_level, equity=.2, probs=cube.probs)]))
            rows = [checker] * (1 if depth == 2 else 4)
            with patch.dict(sys.modules, {'neongammon.text_export': SimpleNamespace(compute_move_notation=lambda *args: '8/6')}):
                actual, _ = self.runtime.dispatch(dict(operation='checker_batch', rows=rows))
                self.assertEqual(self.runtime.analyzer.checker_play.call_count, len(rows))
                self.assertEqual(actual, [self.runtime.dispatch(checker)[0]] * len(rows))
            actual, _ = self.runtime.dispatch(dict(operation='cube_batch_nply', rows=[self.cube]))
            self.runtime.analyzer.cube_action.assert_called_once_with(board=self.cube['board'], beaver=False, **self.cube['context'])
            self.assertEqual(actual, [self.runtime.dispatch(self.cube)[0]])

    def test_cooperative_wheel_batches_deep_cubeful_checkers(self):
        self.runtime.release = PROBE.RELEASES['3.2.1']
        self.runtime.models = object()
        self.runtime.analyzer.checker_play = Mock()
        self.runtime.checker_output = lambda row, result: result
        self.runtime.sage = SimpleNamespace(batch_checker_play=Mock(return_value=['a', 'b']))
        self.runtime.dispatch({'operation': 'begin_batch'})
        for depth in (3, 4):
            self.runtime.profile.update(plies=depth, threads=4)
            rows = [dict(self.cube, operation='checker', dice=dice) for dice in ([2, 2], [4, 1])]
            actual, _ = self.runtime.dispatch(dict(operation='checker_batch', rows=rows))
            self.assertEqual(actual, ['a', 'b'])
            call = self.runtime.sage.batch_checker_play.call_args
            self.assertEqual(call.kwargs['eval_level'], f'{depth}ply')
            self.assertEqual(call.kwargs['n_threads'], 4)
            self.assertEqual([(p['die1'], p['die2']) for p in call.args[0]], [(2, 2), (4, 1)])
        self.runtime.analyzer.checker_play.assert_not_called()

    def test_mixed_jacoby_cubes_use_one_ordered_native_call(self):
        self.runtime.models = object()
        self.runtime.release = PROBE.RELEASES['3.2.0']
        self.runtime.profile.update(plies=3, threads=4)
        rows = [dict(self.cube, context=dict(self.cube['context'], jacoby=flag)) for flag in [True, False, True]]
        cubes = [self.runtime.analyzer.cube_action(None) for _ in rows]
        for i, cube in enumerate(cubes):
            cube.eval_level, cube.equity_nd = '3-ply', i / 10
        batch = Mock(return_value=cubes)
        self.runtime.sage = SimpleNamespace(batch_cube_action_nply=batch)
        self.runtime.dispatch({'operation': 'begin_batch'})
        result, _ = self.runtime.dispatch(dict(operation='cube_batch_nply', rows=rows))
        self.assertEqual([c['no_double'] for c in result], [0, .1, .2])
        self.assertEqual(batch.call_count, 1)
        self.assertEqual([r['jacoby'] for r in batch.call_args.args[0]], [True, False, True])

    def test_rejects_cube_depth_below_profile(self):
        self.runtime.profile = {'plies': 3}
        with self.assertRaisesRegex(ValueError, 'below the requested depth'):
            self.runtime.dispatch(self.cube)
        with self.assertRaisesRegex(ValueError, 'below the requested depth'):
            self.runtime.analyzer.cube_decision = lambda *args, **kwargs: self.decision()
            self.runtime.dispatch(dict(self.cube, decision_only=True))

    @staticmethod
    def decision(**overrides):
        return SimpleNamespace(**dict(eval_level='2-ply', equity_nd=.2, equity_dt=.3, equity_dp=1,
                                      optimal_equity=.3, should_double=True, should_take=True) | overrides)

    def test_decision_only_cube_skips_probabilities_with_the_same_context(self):
        self.runtime.analyzer.cube_action = Mock(wraps=self.runtime.analyzer.cube_action)
        self.runtime.analyzer.cube_decision = Mock(return_value=self.decision())
        decision, cost = self.runtime.dispatch(dict(self.cube, decision_only=True))
        self.assertEqual(decision, {'no_double': .2, 'take': .3, 'pass': 1})
        self.assertEqual(set(cost), {'cpu_us', 'wall_us'})
        self.runtime.analyzer.cube_action.assert_not_called()
        self.runtime.analyzer.cube_decision.assert_called_once_with([0] * 26, beaver=False, **self.cube['context'])
        # The default, and an explicit false, keep the full evaluation.
        for request in (self.cube, dict(self.cube, decision_only=False)):
            full, _ = self.runtime.dispatch(request)
            self.assertEqual(set(full), {'no_double', 'take', 'pass', 'probabilities'})
        self.assertEqual(self.runtime.analyzer.cube_action.call_count, 2)
        self.runtime.analyzer.cube_decision.assert_called_once()

    def test_decision_only_cube_rejects_invalid_scope_and_values(self):
        self.runtime.analyzer.cube_decision = Mock(return_value=self.decision())
        for scope in (1, 'true', None):
            with self.subTest(scope=scope), self.assertRaisesRegex(ValueError, 'Invalid cube decision scope'):
                self.runtime.dispatch(dict(self.cube, decision_only=scope))
        for field in ('equity_nd', 'equity_dt', 'equity_dp', 'optimal_equity'):
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'Invalid cube decision'):
                self.runtime.analyzer.cube_decision.return_value = self.decision(**{field: float('nan')})
                self.runtime.dispatch(dict(self.cube, decision_only=True))
        self.runtime.cubeless = True
        with self.assertRaisesRegex(ValueError, 'no cube decisions'):
            self.runtime.dispatch(dict(self.cube, decision_only=True))

    def test_pre_roll_resignation_uses_the_cube_decision_alone(self):
        self.runtime.analyzer.cube_action = Mock(side_effect=AssertionError('probabilities are unused'))
        self.runtime.analyzer.cube_decision = Mock(return_value=self.decision(equity_nd=-.4))
        self.runtime.analyzer.post_move_analytics = Mock(return_value=SimpleNamespace(
            cubeful_equity=-1.0, probs=SimpleNamespace(to_list=lambda: [0, 0, 0, 0, 0])))
        result, _ = self.runtime.dispatch(dict(self.cube, operation='resignation', dice=[0, 0], level=1))
        self.assertEqual(result, {'accept': True, 'continuation': -.4, 'offered': -1.0})
        self.runtime.analyzer.cube_decision.assert_called_once_with([0] * 26, beaver=False, **self.cube['context'])

    def test_batch_checks_only_boundaries_and_later_single_request_is_protected(self):
        self.assertEqual(self.runtime.dispatch({'operation': 'begin_batch'}), (self.runtime.identity, None))
        first, cost = self.runtime.dispatch(self.cube)
        self.assertEqual(self.runtime.dispatch(self.cube)[0], first)
        self.assertEqual(self.runtime.check_identity.call_count, 1)
        self.assertEqual(self.runtime.dispatch({'operation': 'end_batch'}), (self.runtime.identity, None))
        self.assertEqual(self.runtime.check_identity.call_count, 1)
        # Unchanged metadata of the locked files stands for the complete check.
        self.assertEqual(self.runtime.dispatch(self.cube)[0], first)
        self.assertEqual(self.runtime.check_identity.call_count, 1)
        self.metadata = ('changed',)
        self.current_identity['artifact_fingerprint'] = 'changed'
        with self.assertRaisesRegex(ValueError, 'changed'):
            self.runtime.dispatch(self.cube)
        self.assertEqual(self.runtime.check_identity.call_count, 2)
        self.assertEqual(set(cost), {'cpu_us', 'wall_us'})
        self.assertTrue(all(type(value) is int and value >= 0 for value in cost.values()))

    def test_single_requests_hash_only_after_metadata_changes(self):
        self.runtime.verify()
        self.assertEqual(self.runtime.check_identity.call_count, 1)
        self.runtime.dispatch(self.cube)
        self.runtime.dispatch(self.cube)
        self.assertEqual(self.runtime.check_identity.call_count, 1)
        # A metadata change with identical content passes one complete check
        # and becomes the new guard.
        self.metadata = ('touched',)
        self.runtime.dispatch(self.cube)
        self.assertEqual(self.runtime.check_identity.call_count, 2)
        self.runtime.dispatch(self.cube)
        self.assertEqual(self.runtime.check_identity.call_count, 2)

    def test_change_during_a_single_request_is_rejected_after_it(self):
        self.runtime.verify()
        evaluate = self.runtime.analyzer.cube_action
        def changing(*args, **kwargs):
            self.metadata = ('changed',)
            self.current_identity['artifact_fingerprint'] = 'changed'
            return evaluate(*args, **kwargs)
        self.runtime.analyzer.cube_action = changing
        with self.assertRaisesRegex(ValueError, 'changed during evaluation'):
            self.runtime.dispatch(self.cube)

    def test_missing_guard_runs_the_complete_check(self):
        self.runtime.dispatch(self.cube)
        self.assertEqual(self.runtime.check_identity.call_count, 1)

    def test_changed_identity_is_rejected_at_each_boundary(self):
        for boundary in ('begin_batch', 'end_batch'):
            with self.subTest(boundary=boundary):
                self.setUp()
                if boundary == 'end_batch':
                    self.runtime.dispatch({'operation': 'begin_batch'})
                    self.runtime.dispatch(self.cube)
                self.metadata = ('modified files',)
                self.current_identity['artifact_fingerprint'] = 'changed'
                with self.assertRaisesRegex(ValueError, 'changed'):
                    self.runtime.dispatch({'operation': boundary})

    def test_batch_lifecycle_rejects_nested_begin_and_unmatched_end(self):
        with self.assertRaisesRegex(ValueError, 'No active'):
            self.runtime.dispatch({'operation': 'end_batch'})
        self.runtime.dispatch({'operation': 'begin_batch'})
        with self.assertRaisesRegex(ValueError, 'already active'):
            self.runtime.dispatch({'operation': 'begin_batch'})

    def test_batch_cannot_bypass_normal_identity_request_checks(self):
        self.runtime.dispatch({'operation': 'begin_batch'})
        with self.assertRaisesRegex(ValueError, 'Unsupported integrity batch operation'):
            self.runtime.dispatch({'operation': 'identity'})

    def test_next_batch_rechecks_identity(self):
        self.runtime.dispatch({'operation': 'begin_batch'})
        self.runtime.dispatch({'operation': 'end_batch'})
        self.metadata = ('modified version',)
        self.current_identity['package_version'] = 'changed'
        with self.assertRaisesRegex(ValueError, 'changed'):
            self.runtime.dispatch({'operation': 'begin_batch'})

    def test_real_fingerprint_rejects_metadata_and_asset_identity_changes(self):
        for field in ('RECORD', 'production_model', 'strategy', 'identity_sha256', 'numpy', 'neongammon'):
            for boundary in ('begin_batch', 'end_batch'):
                with self.subTest(field=field, boundary=boundary):
                    self.setUp()
                    del self.runtime.check_identity
                    self.runtime.sage = self.runtime.native = None
                    manifest = dict(identity_sha256='original', neongammon=PROBE.SUPPORTED_VERSIONS[-1],
                                    numpy=PROBE.NUMPY_VERSION, production_model='stage11',
                                    strategy='backgame_pair_phased')
                    self.runtime.integrity['identity'] = lambda *args: dict(manifest)
                    self.runtime.integrity['validate_versions'] = PROBE.validate_versions
                    self.runtime.integrity['ProbeError'] = PROBE.ProbeError
                    distribution = Mock()
                    distribution.read_text.return_value = 'original RECORD'
                    with patch('importlib.metadata.distribution', return_value=distribution):
                        self.runtime.identity = self.runtime.check_identity()
                        if boundary == 'end_batch':
                            self.runtime.dispatch({'operation': 'begin_batch'})
                            self.runtime.dispatch(self.cube)
                        self.metadata = ('modified files',)
                        if field == 'RECORD':
                            distribution.read_text.return_value = 'modified RECORD'
                        else:
                            manifest[field] = 'modified'
                        with self.assertRaises(ValueError):
                            self.runtime.dispatch({'operation': boundary})

class InstalledFilesGuardTests(unittest.TestCase):
    """The real probe integrity functions over a minimal installed wheel."""

    FILES = {
        'neongammon/__init__.py': (
            'from pathlib import Path\n'
            "PRODUCTION_MODEL = 'stage11'\n"
            'class _Weights:\n'
            "    strategy_type = 'backgame_pair_phased'\n"
            "    weight_paths_list = [str(Path(__file__).parent / '_assets/models/contact.weights')]\n"
            'def default_weights():\n'
            '    return _Weights()\n'),
        'neongammon/weights.py': (
            'from pathlib import Path\n'
            'def bearoff_db_path():\n'
            "    return str(Path(__file__).parent / '_assets/bearoff.db')\n"),
        'neongammon/_assets/models/contact.weights': 'weights v1\n',
        'neongammon/_assets/bearoff.db': 'bearoff v1\n',
        'bgbot_cpp.so': 'native v1\n',
    }

    def setUp(self):
        import base64, hashlib, tempfile
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        version = PROBE.SUPPORTED_VERSIONS[-1]
        record = []
        for name, content in self.FILES.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
            digest = base64.urlsafe_b64encode(hashlib.sha256(content.encode()).digest()).decode().rstrip('=')
            record.append(f'{name},sha256={digest},{len(content)}')
        for name, release in (('neongammon', version), ('numpy', PROBE.NUMPY_VERSION)):
            info = self.root / f'{name}-{release}.dist-info'
            info.mkdir()
            (info / 'METADATA').write_text(f'Metadata-Version: 2.1\nName: {name}\nVersion: {release}\n')
            lines = record + [f'{info.name}/METADATA,,', f'{info.name}/RECORD,,'] if name == 'neongammon' else []
            (info / 'RECORD').write_text('\n'.join(lines) + '\n')
        self.record = self.root / f'neongammon-{version}.dist-info/RECORD'

        sys.path.insert(0, str(self.root))
        self.addCleanup(sys.path.remove, str(self.root))
        modules = patch.dict(sys.modules)
        modules.start()
        self.addCleanup(modules.stop)
        for name in [name for name in sys.modules if name == 'neongammon' or name.startswith('neongammon.')]:
            del sys.modules[name]
        sage = importlib.import_module('neongammon')

        spec = importlib.util.spec_from_file_location('sage_worker', PATH)
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        runtime = worker.Runtime.__new__(worker.Runtime)
        runtime.batch_active = False
        runtime.cubeless = False
        runtime.profile = {'plies': 2}
        runtime.sage, runtime.native = sage, SimpleNamespace(__file__=str(self.root / 'bgbot_cpp.so'))
        self.hashes = Mock(wraps=PROBE.identity)
        runtime.integrity = {name: getattr(PROBE, name) for name in (
            'validate_versions', 'ProbeError', 'locked_paths', 'stat_snapshot')}
        runtime.integrity['identity'] = self.hashes
        runtime.integrity['validate_result'] = lambda result, validator: validator(result.probs.to_list())
        runtime.analyzer = SimpleNamespace(cube_action=lambda *args, **kwargs: SimpleNamespace(
            eval_level='2-ply', equity_nd=.2, equity_dt=.3, equity_dp=1,
            probs=SimpleNamespace(to_list=lambda: [.6, .1, .02, .1, .01])))
        runtime.identity = runtime.check_identity()
        runtime.verify()
        self.runtime = runtime
        self.cube = {'operation': 'cube', 'board': [0] * 26,
                     'context': dict(cube_value=1, cube_owner='centered', away1=0, away2=0, is_crawford=False, jacoby=True)}

    def assert_detected(self, change):
        self.runtime.dispatch(self.cube)
        hashed = self.hashes.call_count
        change()
        with self.assertRaises(Exception):
            self.runtime.dispatch(self.cube)
        self.assertEqual(self.hashes.call_count, hashed + 1, 'a changed file must force the complete check')

    def test_unchanged_installation_is_not_hashed_again(self):
        hashed = self.hashes.call_count
        for _ in range(3):
            self.runtime.dispatch(self.cube)
        self.assertEqual(self.hashes.call_count, hashed)

    def test_same_size_modification_is_detected_even_with_restored_mtime(self):
        path = self.root / 'neongammon/_assets/models/contact.weights'
        def modify():
            before = path.stat()
            path.write_text('weights v2\n')
            os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns))
        self.assert_detected(modify)

    def test_size_change_is_detected(self):
        self.assert_detected(lambda: (self.root / 'bgbot_cpp.so').write_text('native v1 patched\n'))

    def test_atomic_replacement_is_detected(self):
        path = self.root / 'neongammon/_assets/bearoff.db'
        def replace():
            before = path.stat()
            replacement = path.with_name('bearoff.tmp')
            replacement.write_text('bearoff v2\n')
            os.utime(replacement, ns=(before.st_atime_ns, before.st_mtime_ns))
            os.replace(replacement, path)
        self.assert_detected(replace)

    def test_record_edit_is_detected(self):
        self.assert_detected(lambda: self.record.write_text(self.record.read_text() + 'extra.txt,,\n'))

    def test_untracked_python_file_is_detected(self):
        self.assert_detected(lambda: (self.root / 'neongammon/extra.py').write_text('pass\n'))

    def test_untracked_file_in_a_nested_package_directory_is_detected(self):
        self.assert_detected(lambda: (self.root / 'neongammon/_assets/models/extra.py').write_text('pass\n'))

    def test_warm_batch_does_not_rehash(self):
        hashed = self.hashes.call_count
        self.runtime.dispatch({'operation': 'begin_batch'})
        self.runtime.dispatch(self.cube)
        self.runtime.dispatch({'operation': 'end_batch'})
        self.assertEqual(self.hashes.call_count, hashed)

    def test_batch_end_rejects_installed_file_changes(self):
        for path in [self.root / 'neongammon/_assets/models/contact.weights',
                     self.record, self.root / 'neongammon/added.py']:
            with self.subTest(path=path):
                original = path.read_bytes() if path.exists() else None
                self.runtime.batch_active = False
                self.runtime.dispatch({'operation': 'begin_batch'})
                hashed = self.hashes.call_count
                path.write_bytes(b'changed')
                with self.assertRaises(Exception):
                    self.runtime.dispatch({'operation': 'end_batch'})
                self.assertEqual(self.hashes.call_count, hashed + 1)
                if original is None:
                    path.unlink()
                else:
                    path.write_bytes(original)

    def test_identical_rewrite_passes_one_complete_check(self):
        path = self.root / 'neongammon/weights.py'
        self.runtime.dispatch(self.cube)
        hashed = self.hashes.call_count
        os.replace(path, path.with_name('moved.py'))
        os.replace(path.with_name('moved.py'), path)
        self.runtime.dispatch(self.cube)
        self.runtime.dispatch(self.cube)
        self.assertEqual(self.hashes.call_count, hashed + 1)


class OpponentPolicyTests(unittest.TestCase):
    def test_repeated_preset_decisions_reuse_analyzer_with_request_seed(self):
        from random import Random
        spec = importlib.util.spec_from_file_location('sage_worker', PATH)
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        runtime = worker.Runtime.__new__(worker.Runtime)
        runtime.profile = {'plies': 1, 'threads': 2}
        runtime.release = PROBE.RELEASES['3.2.0']
        runtime.opponent_policies = {}
        runtime.policy_analyzers = {1: object()}
        def policy(difficulty, seed, parallel_threads, analyzer, cube_analyzer):
            rng = Random(seed)
            self.assertIs(analyzer, runtime.policy_analyzers[1])
            self.assertIs(cube_analyzer, analyzer)
            return SimpleNamespace(roll=rng.random, reseed=rng.seed)
        constructor = Mock(side_effect=policy)
        runtime.sage = SimpleNamespace(DifficultyPolicy=constructor)
        first = runtime.opponent_policy('beginner', 61)
        value = first.roll()
        self.assertIs(runtime.opponent_policy('beginner', 61), first)
        self.assertEqual(first.roll(), value)
        constructor.assert_called_once()

    def test_opponent_requests_use_the_choice_from_the_preset(self):
        spec = importlib.util.spec_from_file_location('sage_worker', PATH)
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        runtime = worker.Runtime.__new__(worker.Runtime)
        runtime.verify = lambda: None
        runtime.batch_active = False
        runtime.cubeless = False
        runtime.profile = {'plies': 1, 'threads': 2}
        before = [0] * 26
        before[6], before[19] = 2, -2
        selected = [0] * 26
        selected[2], selected[19] = 2, -2
        policy = SimpleNamespace(
            select_checker_move=Mock(return_value=selected),
            cube_decision=Mock(return_value=SimpleNamespace(should_double=True, should_take=False)),
        )
        runtime.opponent_policy = Mock(return_value=policy)
        context = dict(cube_value=1, cube_owner='centered', away1=5, away2=3,
                       is_crawford=False, jacoby=False)
        request = dict(board=before, context=context, difficulty='beginner', seed=61)
        checker, _ = runtime.dispatch(dict(request, operation='opponent_checker', dice=[3, 1]))
        cube, _ = runtime.dispatch(dict(request, operation='opponent_cube'))
        self.assertEqual(checker, selected)
        self.assertEqual(cube, {'should_double': True, 'should_take': False})
        runtime.opponent_policy.assert_called_with('beginner', 61)
        policy.select_checker_move.assert_called_once_with(before, 3, 1, beaver=False, **context)
        policy.cube_decision.assert_called_once_with(before, beaver=False, **context)

        runtime.profile['plies'] = 3
        runtime.analyzer = SimpleNamespace(checker_play=Mock(return_value=SimpleNamespace(
            moves=[SimpleNamespace(board=selected)]
        )))
        policy.select_checker_move.reset_mock()
        policy.cube_decision.reset_mock()
        request['difficulty'] = 'full_strength'
        checker, _ = runtime.dispatch(dict(request, operation='opponent_checker', dice=[3, 1]))
        cube, _ = runtime.dispatch(dict(request, operation='opponent_cube'))
        self.assertEqual(checker, selected)
        self.assertEqual(cube, {'should_double': True, 'should_take': False})
        runtime.analyzer.checker_play.assert_called_once_with(before, 3, 1, beaver=False, **context)
        policy.select_checker_move.assert_not_called()
        policy.cube_decision.assert_called_once_with(before, beaver=False, **context)


if __name__ == '__main__':
    unittest.main()
