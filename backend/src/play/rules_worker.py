"""One bounded GNUbg rules process for a play action chain or seeded duel game.

The ordinary adapter owns all game rules and snapshots. This loop only retains
its native state between requests; no checkpoint is committed by the worker.
"""
import ctypes
import json
import os
from pathlib import Path
import resource
import sys
import gnubg

MAX_FRAME = 65536
MAX_ARTIFACT = 32 * 1024 * 1024
resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_ARTIFACT, MAX_ARTIFACT))
flush_native = ctypes.CDLL(None).fflush
# Emit the running GNUbg process's banner before switching native stdout.
flush_native(None)
protocol = os.fdopen(os.dup(1), 'w', buffering=1)
os.dup2(2, 1)
adapter = compile(Path('adapter.py').read_text(), 'adapter.py', 'exec')
protocol.write(json.dumps({'ready': True}) + '\n')
last_id = 0
native_id = None
while True:
    line = sys.stdin.buffer.readline(MAX_FRAME + 1)
    if not line:
        break
    request_id = None
    try:
        if len(line) > MAX_FRAME or not line.endswith(b'\n'):
            raise ValueError('Rule request exceeds its bound')
        frame = json.loads(line)
        request_id = frame['id']
        if type(request_id) is not int or request_id != last_id + 1:
            raise ValueError('Invalid rule request sequence')
        request = frame['request']
        reuse = native_id is not None
        if reuse and (request['new'] or request['native_id'] != native_id or gnubg.gnubgid() != native_id):
            raise ValueError('Rule checkpoint changed; discard the action chain')
        for name in ('snapshot.json', 'output.sgf', 'native-output.txt'):
            Path(name).unlink(missing_ok=True)
        Path('request.json').write_text(json.dumps(request, allow_nan=False))
        with open('native-output.txt', 'w') as capture:
            os.dup2(capture.fileno(), 1)
            try:
                exec(adapter, {'__name__': 'neondojo_rules', '_reuse_checkpoint': reuse})
            finally:
                sys.stdout.flush()
                flush_native(None)
                os.dup2(2, 1)
        snapshot = json.loads(Path('snapshot.json').read_text())
        native_id = snapshot['native_id']
        if native_id != gnubg.gnubgid() or not 0 < Path('output.sgf').stat().st_size <= MAX_ARTIFACT:
            raise ValueError('Rule adapter returned an inconsistent checkpoint')
        last_id = request_id
        protocol.write(json.dumps({'id': request_id, 'ok': True}) + '\n')
    except Exception as error:
        protocol.write(json.dumps({'id': request_id, 'error': str(error)[:2000]}) + '\n')
        break
