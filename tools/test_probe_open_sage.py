"""Dependency-free validation tests; run with python3 -m unittest discover -s tools -p 'test_probe_open_sage.py'."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('probe_open_sage', Path(__file__).with_name('probe_open_sage.py'))
probe = importlib.util.module_from_spec(SPEC)


class ProbeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not Path(SPEC.origin).exists():
            return
        SPEC.loader.exec_module(probe)

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

    def test_thread_budget_rejects_silent_oversubscription(self):
        self.assertTrue(hasattr(probe, 'validate_thread_budget'), 'thread budget validator missing')
        probe.validate_thread_budget(2, [2])
        with self.assertRaisesRegex(probe.ProbeError, 'requested 2.*10'):
            probe.validate_thread_budget(2, [10])

    def test_network_guard_denies_socket_connections(self):
        self.assertTrue(hasattr(probe, 'deny_network'), 'network guard missing')
        result = subprocess.run([sys.executable, '-c', 'from probe_open_sage import deny_network; import socket; deny_network(); socket.create_connection(("127.0.0.1", 9))'], cwd=Path(__file__).parent, capture_output=True, text=True, timeout=15)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Network access forbidden', result.stderr)


if __name__ == '__main__':
    unittest.main()
