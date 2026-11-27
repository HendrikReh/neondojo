"""Opt-in scalar/batch parity check. Run with the locked NeonGammon interpreter."""
import importlib.util
import json
import os
import subprocess
import sys
import unittest
from importlib.metadata import version
from pathlib import Path

import neongammon as ng


ROOT = Path(__file__).resolve().parents[2]
PROBE_PATH = ROOT / 'tools/probe_open_sage.py'


def _load_probe_module():
    spec = importlib.util.spec_from_file_location('probe_open_sage', PROBE_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PROBE = _load_probe_module()
RELEASES = PROBE.RELEASES


def fixture():
    policy = ng.BgBotAnalyzer(eval_level='1ply', parallel_threads=1)
    game = ng.simulate_game(seed=42, config=ng.GameConfig(match_length=5), players=(policy, policy))
    checkers, cubes = [], []
    for turn in game['turns']:
        context = {k: turn[k] for k in ('cube_value', 'cube_owner', 'away1', 'away2', 'is_crawford', 'jacoby')}
        if checkers:  # The opening roll has no pre-roll cube decision.
            cubes.append(dict(operation='cube', board=turn['board'], context=context))
        checker_context = dict(context, cube_value=turn['checker_cube_value'], cube_owner=turn['checker_cube_owner'])
        checkers.append(dict(operation='checker', board=turn['board'], dice=turn['dice'],
                             force_board=turn['post_board'], context=checker_context))
    return checkers, cubes


def sidecar_replies(depth, identity, frames):
    probe = PROBE_PATH.read_text()
    worker = (ROOT / 'backend/src/engines/worker.py').read_text()
    source = (f"integrity = {{'__name__': 'neondojo_integrity', 'RELEASES': {RELEASES!r}}}\n"
              f"exec({probe!r}, integrity)\n{worker}\nmain(integrity)")
    profile = dict(plies=depth, threads=4, artifact_fingerprint=identity['artifact_fingerprint'])
    requests = [dict(operation='initialize', profile=profile), dict(operation='begin_batch'),
                *frames, dict(operation='end_batch')]
    wire = ''.join(json.dumps(dict(request, id=i)) + '\n' for i, request in enumerate(requests, 1))
    env = os.environ | PROBE.SINGLE_THREADED_MATH
    process = subprocess.run([sys.executable, '-I', '-u', '-c', source], input=wire, text=True,
                             capture_output=True, env=env, timeout=900, check=True)
    replies = [json.loads(line) for line in process.stdout.splitlines()]
    for i, reply in enumerate(replies, 1):
        if 'error' in reply or reply.get('id') != i:
            raise ValueError(f"Unexpected {depth}-ply sidecar reply for request {i}: {reply}")
    if len(replies) != len(requests):
        raise ValueError(f"Sidecar returned {len(replies)} of {len(requests)} replies: {process.stderr[-1000:]}")
    if any(replies[i]['result'] != identity for i in (0, 1, -1)):
        raise ValueError('Engine identity changed at a batch boundary')
    return [reply['result'] for reply in replies[2:-1]]


class GameParity(unittest.TestCase):
    def test_all_decisions(self):
        checkers, cubes = fixture()
        self.assertEqual((len(checkers), len(cubes)), (44, 43))
        self.assertIn(133, [len(ng.possible_moves(row['board'], 2, 2))
                            for row in checkers if row['dice'] == [2, 2]])
        package = version('neongammon')
        identity = dict(package_version=package, artifact_fingerprint=RELEASES[package]['fingerprint'])
        frames = [dict(operation=operation, rows=rows[i:i + 4])
                  for operation, rows in [('checker_batch', checkers), ('cube_batch_nply', cubes)]
                  for i in range(0, len(rows), 4)]
        for depth in (3, 4):
            with self.subTest(depth=depth):
                scalar = sidecar_replies(depth, identity, checkers + cubes)
                batched = sidecar_replies(depth, identity, frames)
                offset = 0
                for frame, results in zip(frames, batched, strict=True):
                    end = offset + len(frame['rows'])
                    self.assertEqual(results, scalar[offset:end], f"{depth}-ply {frame['operation']} at row {offset}")
                    offset = end
                self.assertEqual(offset, 87)
                print(f"{depth}-ply: 44 checker and 43 cube results match in every field", flush=True)


if __name__ == '__main__':
    unittest.main()
