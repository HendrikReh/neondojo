"""Dependency-free validation tests; run with python3 -m unittest discover -s tools -p 'test_probe_open_sage.py'."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import base64
import hashlib
from types import SimpleNamespace
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('probe_open_sage', Path(__file__).with_name('probe_open_sage.py'))
probe = importlib.util.module_from_spec(SPEC)


class ProbeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not Path(SPEC.origin).exists():
            return
        SPEC.loader.exec_module(probe)

    def test_accepts_only_supported_package_versions(self):
        for version in probe.SUPPORTED_VERSIONS:
            probe.validate_versions({'neongammon': version, 'numpy': probe.NUMPY_VERSION})
        for value in ({'neongammon': '2.0.20261118', 'numpy': probe.NUMPY_VERSION},
                      {'neongammon': probe.SUPPORTED_VERSIONS[-1], 'numpy': '2.5.3'}):
            with self.subTest(value=value), self.assertRaises(probe.ProbeError):
                probe.validate_versions(value)

    def test_valid_nested_probabilities(self):
        self.assertTrue(hasattr(probe, 'validate_probabilities'), 'probability validator missing')
        probe.validate_probabilities([0.6, 0.2, 0.03, 0.1, 0.01])

    def test_rejects_nonfinite_and_inconsistent_probabilities(self):
        self.assertTrue(hasattr(probe, 'validate_probabilities'), 'probability validator missing')
        for values in ([float('nan'), 0, 0, 0, 0], [0.4, .5, 0, 0, 0], [.7, 0, 0, .4, 0], [.5, .1, .2, 0, 0], [.5, 0, 0, .1, .2], [1.1, 0, 0, 0, 0]):
            with self.subTest(values=values), self.assertRaises(probe.ProbeError):
                probe.validate_probabilities(values)

    def test_wrong_architecture_has_actionable_error(self):
        self.assertTrue(hasattr(probe, 'validate_platform'), 'platform validator missing')
        with self.assertRaisesRegex(probe.ProbeError, 'ARM64.*Python 3.12'):
            probe.validate_platform('Darwin', 'x86_64', (3, 12))
        probe.validate_platform('Darwin', 'arm64', (3, 12))

    def test_missing_asset_fails_without_private_path(self):
        self.assertTrue(hasattr(probe, 'hash_required_file'), 'asset validator missing')
        with self.assertRaisesRegex(probe.ProbeError, 'Reinstall.*locked wheels') as caught:
            probe.hash_required_file(Path('/private/example/missing.weights'), 'models/missing.weights')
        self.assertNotIn('/private/example', str(caught.exception))

    def test_hash_changes_when_asset_changes(self):
        self.assertTrue(hasattr(probe, 'hash_required_file'), 'asset validator missing')
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'model'
            path.write_bytes(b'first')
            first = probe.hash_required_file(path, 'model')
            path.write_bytes(b'second')
            self.assertNotEqual(first, probe.hash_required_file(path, 'model'))

    def test_identity_hashes_each_path_once_but_rechecks_next_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = root / 'neongammon'
            models = package / '_assets/models'
            models.mkdir(parents=True)
            source, model, native = package / '__init__.py', models / 'model', root / 'native.so'
            for path, data in ((source, b'source'), (model, b'weights'), (native, b'native')):
                path.write_bytes(data)
            files = {'__init__.py': hashlib.sha256(b'source').hexdigest(),
                     '_assets/models/model': hashlib.sha256(b'weights').hexdigest(),
                     'native.so': hashlib.sha256(b'native').hexdigest()}
            records = []
            for label, digest in files.items():
                entry = __import__('importlib.metadata', fromlist=['PackagePath']).PackagePath(
                    label if label == 'native.so' else 'neongammon/' + label)
                entry.hash = SimpleNamespace(mode='sha256', value=base64.urlsafe_b64encode(bytes.fromhex(digest)).decode().rstrip('='))
                records.append(entry)
            sage = SimpleNamespace(__file__=str(source), PRODUCTION_MODEL='fixture',
                                   default_weights=lambda: SimpleNamespace(weight_paths_list=[str(model)] * 24, strategy_type='fixture'))
            weights = SimpleNamespace(bearoff_db_path=lambda: str(model))
            with patch.dict(sys.modules, {'neongammon.weights': weights}), \
                 patch.object(probe.importlib.metadata, 'distribution', return_value=SimpleNamespace(files=records)), \
                 patch.object(probe.importlib.metadata, 'version', return_value='fixture'), \
                 patch.object(probe, 'hash_required_file', wraps=probe.hash_required_file) as hashing:
                result = probe.identity(sage, SimpleNamespace(__file__=str(native)))
                self.assertEqual(result['files_sha256'], files)
                self.assertEqual(result['identity_sha256'], hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest())
                self.assertEqual(hashing.call_count, 3, 'Repeated model slots must reuse this check\'s digest')
                missing = records.pop()
                with self.assertRaisesRegex(probe.ProbeError, 'Missing SHA256 wheel RECORD'):
                    probe.identity(sage, SimpleNamespace(__file__=str(native)))
                records.append(missing)
                records[0].hash.mode = 'md5'
                with self.assertRaisesRegex(probe.ProbeError, 'Missing SHA256 wheel RECORD'):
                    probe.identity(sage, SimpleNamespace(__file__=str(native)))
                records[0].hash.mode = 'sha256'
                model.write_bytes(b'changed')
                with self.assertRaisesRegex(probe.ProbeError, 'integrity mismatch'):
                    probe.identity(sage, SimpleNamespace(__file__=str(native)))
                model.unlink()
                with self.assertRaisesRegex(probe.ProbeError, 'Missing model assets'):
                    probe.identity(sage, SimpleNamespace(__file__=str(native)))

    def test_missing_dependency_subprocess_produces_failed_report(self):
        self.assertTrue(Path(SPEC.origin).exists(), 'probe command missing')
        result = subprocess.run([sys.executable, '-S', SPEC.origin, '--worker', '--skip-platform-check'], capture_output=True, text=True, timeout=15)
        self.assertNotEqual(result.returncode, 0)
        report = json.loads(result.stdout)
        self.assertEqual(report['status'], 'failed')
        self.assertIn('Install', report['error'])

    def test_corrupt_asset_record_is_rejected(self):
        self.assertTrue(hasattr(probe, 'validate_record_digest'), 'wheel RECORD integrity validator missing')
        # SHA256 of the literal bytes b'hello', from a fixed independent fixture.
        digest = '2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824'
        probe.validate_record_digest(digest, 'LPJNul-wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ', 'model')
        with self.assertRaisesRegex(probe.ProbeError, 'integrity mismatch'):
            probe.validate_record_digest('0' * 64, 'LPJNul-wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ', 'model')

    def test_timeout_is_failure_with_actionable_report(self):
        result = subprocess.run([sys.executable, SPEC.origin, '--timeout', '0.000001'], capture_output=True, text=True, timeout=15)
        self.assertNotEqual(result.returncode, 0)
        report = json.loads(result.stdout)
        self.assertEqual(report['status'], 'failed')
        self.assertIn('timed out', report['error'])

    def test_cli_exposes_explicit_level_subset(self):
        result = subprocess.run([sys.executable, SPEC.origin, '--help'], capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0)
        self.assertIn('--levels', result.stdout)

    def test_serial_capability_requires_zero_candidate_pools(self):
        probe.validate_thread_budget(2, [], serial=True)
        with self.assertRaises(probe.ProbeError):
            probe.validate_thread_budget(2, [2], serial=True)
        with self.assertRaises(probe.ProbeError):
            probe.validate_thread_budget(2, [])

    def test_thread_budget_rejects_silent_oversubscription(self):
        self.assertTrue(hasattr(probe, 'validate_thread_budget'), 'thread budget validator missing')
        probe.validate_thread_budget(2, [2])
        with self.assertRaisesRegex(probe.ProbeError, 'requested 2.*10'):
            probe.validate_thread_budget(2, [10])

    def test_thread_budget_observer_records_pools_and_native_routine(self):
        calls = []

        def checker_play_nply(*args, **kwargs):
            calls.append(kwargs)
            return ['native result']

        class Pool:
            def __init__(self, max_workers=None):
                self.max_workers = max_workers

        native = SimpleNamespace(checker_play_nply=checker_play_nply)
        module = SimpleNamespace(bgbot_cpp=native, ThreadPoolExecutor=Pool)
        sage = SimpleNamespace(analyzer=module)

        def analysis():
            module.ThreadPoolExecutor(max_workers=3)
            return native.checker_play_nply([0] * 26, 3, 1, n_threads=4)

        result, observed = probe.observe_thread_budgets(sage, analysis)
        self.assertEqual(result, ['native result'])
        self.assertEqual(observed, [3, 4])
        self.assertEqual(calls, [{'n_threads': 4}])
        # Both observers are removed again after the call.
        self.assertIs(module.ThreadPoolExecutor, Pool)
        self.assertIs(native.checker_play_nply, checker_play_nply)

    def test_network_guard_denies_socket_connections(self):
        self.assertTrue(hasattr(probe, 'deny_network'), 'network guard missing')
        result = subprocess.run([sys.executable, '-c', 'from probe_open_sage import deny_network; import socket; deny_network(); socket.create_connection(("127.0.0.1", 9))'], cwd=Path(__file__).parent, capture_output=True, text=True, timeout=15)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Network access forbidden', result.stderr)


if __name__ == '__main__':
    unittest.main()
