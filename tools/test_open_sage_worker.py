"""Dependency-free tests for the bundled NeonGammon sidecar protocol."""
import importlib.util
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
        self.runtime.integrity = {'validate_result': lambda result, validator: validator(result.probs.to_list())}
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

    def test_rejects_cube_depth_below_profile(self):
        self.runtime.profile = {'plies': 3}
        with self.assertRaisesRegex(ValueError, 'below the requested depth'):
            self.runtime.dispatch(self.cube)

    def test_batch_checks_only_boundaries_and_later_single_request_is_protected(self):
        self.assertEqual(self.runtime.dispatch({'operation': 'begin_batch'}), (self.runtime.identity, None))
        first, cost = self.runtime.dispatch(self.cube)
        self.assertEqual(self.runtime.dispatch(self.cube)[0], first)
        self.assertEqual(self.runtime.check_identity.call_count, 1)
        self.assertEqual(self.runtime.dispatch({'operation': 'end_batch'}), (self.runtime.identity, None))
        self.assertEqual(self.runtime.check_identity.call_count, 2)
        self.assertEqual(self.runtime.dispatch(self.cube)[0], first)
        self.assertEqual(self.runtime.check_identity.call_count, 4)
        self.current_identity['artifact_fingerprint'] = 'changed'
        with self.assertRaisesRegex(ValueError, 'changed'):
            self.runtime.dispatch(self.cube)
        self.assertEqual(set(cost), {'cpu_us', 'wall_us'})
        self.assertTrue(all(type(value) is int and value >= 0 for value in cost.values()))

    def test_changed_identity_is_rejected_at_each_boundary(self):
        for boundary in ('begin_batch', 'end_batch'):
            with self.subTest(boundary=boundary):
                self.setUp()
                if boundary == 'end_batch':
                    self.runtime.dispatch({'operation': 'begin_batch'})
                    self.runtime.dispatch(self.cube)
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
                        if field == 'RECORD':
                            distribution.read_text.return_value = 'modified RECORD'
                        else:
                            manifest[field] = 'modified'
                        with self.assertRaises(ValueError):
                            self.runtime.dispatch({'operation': boundary})

class OpponentPolicyTests(unittest.TestCase):
    def test_repeated_preset_decisions_reuse_analyzer_with_request_seed(self):
        from random import Random
        spec = importlib.util.spec_from_file_location('sage_worker', PATH)
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        runtime = worker.Runtime.__new__(worker.Runtime)
        runtime.profile = {'plies': 1, 'threads': 2}
        runtime.opponent_policies = {}
        constructor = Mock(side_effect=lambda difficulty, seed, parallel_threads:
                           SimpleNamespace(_rng=Random(seed)))
        module = SimpleNamespace(DifficultyPolicy=constructor)
        with patch.dict(sys.modules, {'neondojo_opponent_policy': module}):
            first = runtime.opponent_policy('beginner', 61)
            value = first._rng.random()
            self.assertIs(runtime.opponent_policy('beginner', 61), first)
            self.assertEqual(first._rng.random(), value)
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


if __name__ == '__main__':
    unittest.main()
