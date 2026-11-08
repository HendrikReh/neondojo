import argparse
import copy
import errno
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import urllib.error
import uuid
from contextlib import redirect_stdout

import rebuild_demo as demo


class DemoGuardTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.backup = self.root / 'backup.dump'
        self.backup.write_bytes(b'verified backup')
        self.manifest = self.root / 'manifest.json'
        self.identity = {
            'system_identifier': '123456789',
            'database_oid': '16384',
            'database_name': 'bgtutor',
        }
        self.report = {
            'ready': True,
            'activated': False,
            'backup': str(self.backup),
            'database_identity': self.identity,
            'databases': {kind: str(uuid.uuid4()) for kind in ['bgf', 'mat', 'xg']},
            'files': [],
        }
        for kind, database_id in self.report['databases'].items():
            self.report['files'].append({
                'path': f'{kind}/example.{kind}',
                'sha256': 'a' * 64,
                'http_status': 201,
                'result': {'match': {'id': str(uuid.uuid4()), 'database_id': database_id}},
            })
        self.save()

    def save(self, report=None):
        self.manifest.write_text(json.dumps(self.report if report is None else report))
        self.manifest.chmod(0o600)

    def offline(self):
        return patch.object(demo.urllib.request, 'urlopen', side_effect=urllib.error.URLError(ConnectionRefusedError(errno.ECONNREFUSED, 'refused')))

    def test_activation_rejects_transport_failure_that_does_not_prove_offline(self):
        for reason in [TimeoutError('timeout'), OSError(errno.EHOSTUNREACH, 'unreachable')]:
            with self.subTest(reason=reason), patch.object(demo.urllib.request, 'urlopen', side_effect=urllib.error.URLError(reason)), patch.object(demo, 'sql', return_value='') as sql:
                self.save()
                with self.assertRaises(SystemExit):
                    demo.activate(argparse.Namespace(manifest=self.manifest))
                sql.assert_not_called()
                self.assertFalse(json.loads(self.manifest.read_text())['activated'])

    def test_activation_rejects_unsafe_manifest_before_sql(self):
        reports = []
        duplicate = copy.deepcopy(self.report)
        duplicate['databases']['mat'] = duplicate['databases']['bgf']
        reports.append(duplicate)
        default = copy.deepcopy(self.report)
        default['databases']['bgf'] = demo.DEFAULT
        reports.append(default)
        empty_folder = copy.deepcopy(self.report)
        empty_folder['files'] = empty_folder['files'][:-1]
        reports.append(empty_folder)
        missing_identity = copy.deepcopy(self.report)
        del missing_identity['database_identity']
        reports.append(missing_identity)
        for report in reports:
            with self.subTest(report=report), self.offline(), patch.object(demo, 'sql', return_value='') as sql:
                self.save(report)
                with self.assertRaises(SystemExit):
                    demo.activate(argparse.Namespace(manifest=self.manifest))
                sql.assert_not_called()

    def test_stage_rejects_api_container_mismatch_before_backup_or_import(self):
        for kind in ['bgf', 'mat', 'xg']:
            (self.root / 'games' / kind).mkdir(parents=True)
            (self.root / 'games' / kind / f'example.{kind}').write_bytes(b'source')
        snapshot = {'identity': self.identity, 'databases': [demo.DEFAULT]}
        with patch.object(demo, 'request', return_value=(200, [{'id': str(uuid.uuid4())}])) as request, patch.object(demo, 'sql', return_value=json.dumps(snapshot)), patch.object(demo.subprocess, 'run') as run:
            with self.assertRaises(SystemExit):
                demo.stage(argparse.Namespace(games=self.root / 'games', output=self.root / 'output', allow_rejected=False))
            self.assertEqual(request.call_args_list[0].args, ('/databases',))
            self.assertEqual(request.call_count, 1)
            run.assert_not_called()

    def test_activation_preserves_manifest_if_atomic_replace_fails(self):
        original = self.manifest.read_bytes()
        with self.offline(), patch.object(demo, 'sql', return_value=''), patch('os.replace', side_effect=OSError('disk failure')):
            with self.assertRaises(OSError):
                demo.activate(argparse.Namespace(manifest=self.manifest))
        self.assertEqual(self.manifest.read_bytes(), original)
        self.assertEqual(self.manifest.stat().st_mode & 0o777, 0o600)

    def test_legacy_import_response_derives_database_from_verified_folder(self):
        for item in self.report['files']:
            del item['result']['match']['database_id']
        self.save()
        with self.offline(), patch.object(demo, 'sql', return_value='') as sql, redirect_stdout(io.StringIO()):
            demo.activate(argparse.Namespace(manifest=self.manifest))
        sql.assert_called_once()

    def test_successful_stage_records_cluster_identity_and_checks_created_ids(self):
        for kind in ['bgf', 'mat', 'xg']:
            (self.root / 'games' / kind).mkdir(parents=True)
            (self.root / 'games' / kind / f'example.{kind}').write_bytes(kind.encode())
        database_ids = [demo.DEFAULT]
        def request(path, body=None, content_type=None):
            if path == '/databases' and body is None:
                return 200, [{'id': database_id} for database_id in database_ids]
            if path == '/databases':
                database_ids.append(str(uuid.uuid4()))
                return 201, {'id': database_ids[-1]}
            if path.startswith('/imports?'):
                return 201, {'match': {'id': str(uuid.uuid4())}}
            return 200, {'records': 0}
        with patch.object(demo, 'request', side_effect=request), patch.object(demo, 'sql', side_effect=lambda statement: json.dumps({'identity': self.identity, 'databases': database_ids})), patch.object(demo.subprocess, 'run'), redirect_stdout(io.StringIO()):
            demo.stage(argparse.Namespace(games=self.root / 'games', output=self.root / 'output', allow_rejected=False))
        [manifest] = (self.root / 'output').glob('demo-import-*.json')
        report = json.loads(manifest.read_text())
        self.assertEqual(report['database_identity'], self.identity)
        self.assertEqual(len(report['files']), 3)
        self.assertTrue(report['ready'])
        self.assertEqual(manifest.stat().st_mode & 0o777, 0o600)

    def test_staging_cannot_mark_an_entirely_rejected_folder_ready(self):
        for kind in ['bgf', 'mat', 'xg']:
            (self.root / 'games' / kind).mkdir(parents=True)
            (self.root / 'games' / kind / f'example.{kind}').write_bytes(kind.encode())
        database_ids = [demo.DEFAULT]
        def request(path, body=None, content_type=None):
            if path == '/databases' and body is None:
                return 200, [{'id': database_id} for database_id in database_ids]
            if path == '/databases':
                database_ids.append(str(uuid.uuid4()))
                return 201, {'id': database_ids[-1]}
            return 422, {'error': 'strict format rejection'}
        with patch.object(demo, 'request', side_effect=request), patch.object(demo, 'sql', side_effect=lambda statement: json.dumps({'identity': self.identity, 'databases': database_ids})), patch.object(demo.subprocess, 'run'), redirect_stdout(io.StringIO()):
            with self.assertRaises(SystemExit):
                demo.stage(argparse.Namespace(games=self.root / 'games', output=self.root / 'output', allow_rejected=True))
        [manifest] = (self.root / 'output').glob('demo-import-*.json')
        self.assertFalse(json.loads(manifest.read_text()).get('ready', False))


@unittest.skipUnless(os.environ.get('BGTUTOR_REBUILD_PG_TESTS') == '1', 'optional isolated PostgreSQL fixtures')
class DemoPostgresGuardTests(unittest.TestCase):
    save = DemoGuardTests.save

    def setUp(self):
        DemoGuardTests.setUp(self)
        self.database = 'demo_guard_test_'+uuid.uuid4().hex
        subprocess.run(['docker', 'exec', demo.CONTAINER, 'createdb', '-U', 'bgtutor', self.database], check=True, capture_output=True)
        self.addCleanup(lambda: subprocess.run(['docker', 'exec', demo.CONTAINER, 'dropdb', '-U', 'bgtutor', '--force', self.database], check=True, capture_output=True))
        self.run_sql('''CREATE TABLE databases(id uuid PRIMARY KEY,name text,description text,version bigint DEFAULT 0);
CREATE TABLE matches(id uuid PRIMARY KEY,database_id uuid REFERENCES databases(id),sha256 text);
CREATE TABLE learning_history(id integer);
INSERT INTO learning_history VALUES(1);''')
        self.report['database_identity'] = json.loads(self.run_sql(demo.IDENTITY_SQL+';'))
        for id in [demo.DEFAULT, *self.report['databases'].values()]:
            self.run_sql(f"INSERT INTO databases(id,name) VALUES('{id}','Collection');")
        self.old_match = str(uuid.uuid4())
        self.run_sql(f"INSERT INTO matches VALUES('{self.old_match}','{demo.DEFAULT}','{'b'*64}');")
        for item in self.report['files']:
            match = item['result']['match']
            self.run_sql(f"INSERT INTO matches VALUES('{match['id']}','{match['database_id']}','{item['sha256']}');")

    def run_sql(self, statement):
        # This helper cannot be redirected to the real application database.
        assert self.database.startswith('demo_guard_test_')
        return subprocess.run(['docker', 'exec', '-i', demo.CONTAINER, 'psql', '-U', 'bgtutor', '-d', self.database, '-v', 'ON_ERROR_STOP=1', '-At'], input=statement, text=True, check=True, capture_output=True).stdout

    def snapshot(self):
        return self.run_sql("SELECT jsonb_build_object('databases',(SELECT jsonb_agg(to_jsonb(d) ORDER BY id) FROM databases d),'matches',(SELECT jsonb_agg(to_jsonb(m) ORDER BY id) FROM matches m),'learning',(SELECT jsonb_agg(id) FROM learning_history));")

    def assert_rejected_without_changes(self, report):
        before = self.snapshot()
        with self.assertRaises(subprocess.CalledProcessError):
            self.run_sql(demo.activation_statement(report))
        self.assertEqual(self.snapshot(), before)

    def test_wrong_cluster_or_database_identity_rolls_back(self):
        for key in ['system_identifier', 'database_oid', 'database_name']:
            report = copy.deepcopy(self.report)
            report['database_identity'][key] = '12345' if key != 'database_name' else 'other'
            self.assert_rejected_without_changes(report)

    def test_changed_digest_database_or_missing_match_rolls_back(self):
        item = self.report['files'][0]
        id = item['result']['match']['id']
        original = item['result']['match']['database_id']
        for change in [f"UPDATE matches SET sha256='changed' WHERE id='{id}'", f"UPDATE matches SET database_id='{demo.DEFAULT}' WHERE id='{id}'", f"DELETE FROM matches WHERE id='{id}'"]:
            self.run_sql(change+';')
            self.assert_rejected_without_changes(self.report)
            self.run_sql(f"DELETE FROM matches WHERE id='{id}'; INSERT INTO matches VALUES('{id}','{original}','{item['sha256']}');")

    def test_missing_staging_database_rolls_back(self):
        id = self.report['databases']['bgf']
        self.run_sql(f"DELETE FROM matches WHERE database_id='{id}'; DELETE FROM databases WHERE id='{id}';")
        self.assert_rejected_without_changes(self.report)

    def test_unexpected_staged_match_rolls_back(self):
        self.run_sql(f"INSERT INTO matches VALUES('{uuid.uuid4()}','{self.report['databases']['bgf']}','{'c'*64}');")
        self.assert_rejected_without_changes(self.report)

    def test_swap_preserves_learning_and_a_retry_cannot_wipe_activated_bgf(self):
        statement = demo.activation_statement(self.report)
        self.run_sql(statement)
        self.assertEqual(self.run_sql('SELECT count(*) FROM matches;').strip(), '3')
        self.assertEqual(self.run_sql('SELECT count(*) FROM databases;').strip(), '3')
        self.assertEqual(self.run_sql('SELECT count(*) FROM learning_history;').strip(), '1')
        self.assertEqual(self.run_sql(f"SELECT count(*) FROM matches WHERE database_id='{demo.DEFAULT}';").strip(), '1')
        self.assert_rejected_without_changes(self.report)


if __name__ == '__main__':
    unittest.main()
