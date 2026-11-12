"""Bounded GNUbg rule-session protocol; no engine dependency."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

WORKER = Path(__file__).resolve().parents[1] / 'backend/src/play/rules_worker.py'
FAKE_ADAPTER = '''import json, gnubg
with open('request.json') as f: request = json.load(f)
if not globals().get('_reuse_checkpoint', False): gnubg.loads += 1
if request['action'] == 'bad': raise RuntimeError('invalid action')
if request['action'] == 'omit': pass
else:
    if request['action'] == 'move': gnubg.current = 'after'
    with open('snapshot.json','w') as f: json.dump({'native_id':gnubg.current,'loads':gnubg.loads},f)
    with open('output.sgf','w') as f: f.write('checkpoint')
    print('native noise')
'''

class RulesProtocolTests(unittest.TestCase):
    def run_worker(self, frames):
        self.assertTrue(WORKER.exists(), 'persistent rule worker is missing')
        with tempfile.TemporaryDirectory() as directory:
            Path(directory,'adapter.py').write_text(FAKE_ADAPTER)
            script = """import sys,types
module=types.ModuleType('gnubg');module.current='before';module.loads=0
module.gnubgid=lambda:module.current
sys.modules['gnubg']=module
""" + WORKER.read_text()
            result=subprocess.run([sys.executable,'-I','-u','-c',script],cwd=directory,input=frames,capture_output=True,timeout=5)
            snapshots=json.loads(Path(directory,'snapshot.json').read_text()) if Path(directory,'snapshot.json').exists() else None
            return [json.loads(line) for line in result.stdout.splitlines()],snapshots

    def request(self, number, action='inspect', native_id='before'):
        return (json.dumps({'id':number,'request':{'new':False,'action':action,'native_id':native_id}})+'\n').encode()

    def test_inspect_and_move_share_one_loaded_checkpoint(self):
        messages,snapshot=self.run_worker(self.request(1)+self.request(2,'move'))
        self.assertTrue(messages[0]['ready'])
        self.assertEqual([m['id'] for m in messages[1:]], [1,2])
        self.assertTrue(all(m['ok'] for m in messages[1:]))
        self.assertEqual(snapshot,{'native_id':'after','loads':1})

    def test_stale_checkpoint_and_repeated_id_fail_closed(self):
        for second in [self.request(2,'move','wrong'),self.request(1,'move')]:
            with self.subTest(second=second):
                messages,_=self.run_worker(self.request(1)+second+self.request(3))
                self.assertEqual(len(messages),3)
                self.assertIn('error',messages[-1])

    def test_bad_request_missing_result_and_oversized_frame_are_fatal(self):
        for frame in [b'not json\n',b'x'*65537+b'\n',self.request(2,'bad'),self.request(2,'omit')]:
            with self.subTest(frame_length=len(frame)):
                messages,_=self.run_worker(self.request(1)+frame+self.request(3))
                self.assertEqual(len(messages),3)
                self.assertIn('error',messages[-1])

if __name__=='__main__': unittest.main()
