"""Dependency-free tests for the bundled Open Sage sidecar protocol."""
import importlib.util
from pathlib import Path
import unittest
import subprocess
import sys
sys.dont_write_bytecode = True
import json

PATH = Path(__file__).resolve().parents[1] / 'backend/src/engines/worker.py'

class WorkerTests(unittest.TestCase):
    def test_worker_exists(self):
        self.assertTrue(PATH.exists(), 'bundled sidecar is missing')

    def test_rejects_unbounded_or_unvalidated_profiles(self):
        if not PATH.exists():
            self.fail('bundled sidecar is missing')
        spec = importlib.util.spec_from_file_location('sage_worker', PATH)
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        worker.validate_profile(2, 2)
        worker.validate_profile(2, 4)
        for plies, threads in [(1, 2), (3, 2), (2, 0), (2, 3), (2, 100)]:
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
        return request['value']
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

    def test_malformed_and_oversized_requests_terminate_worker(self):
        for payload in (b'not json\n', b'x' * 262145, b'{"id":-1}\n'):
            with self.subTest(payload_length=len(payload)):
                result = self.run_protocol(payload)
                self.assertIn('error', json.loads(result.stdout))

    def test_nonfinite_response_is_rejected(self):
        result = self.run_protocol(b'{"id":1,"operation":"initialize","profile":{}}\n{"id":2,"operation":"test","value":NaN}\n')
        response = json.loads(result.stdout.splitlines()[-1])
        self.assertIn('error', response)

if __name__ == '__main__':
    unittest.main()
