#!/usr/bin/env python3
"""Stage private demos through REST, then activate while the backend is stopped.

Data, manifests and backups always remain outside this Git repository.
"""
import argparse
import errno
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
import uuid
from datetime import datetime
from pathlib import Path
from zoneinfo import ZoneInfo

REPO = Path(__file__).resolve().parents[1]
DATA = Path.home() / 'Library/Application Support/NeonDojo'
DEFAULT = '00000000-0000-0000-0000-000000000001'
API = 'http://127.0.0.1:8787/api'
CONTAINER = 'neondojo-db-1'
IDENTITY_SQL = """SELECT jsonb_build_object(
    'system_identifier',s.system_identifier::text,
    'database_oid',d.oid::text,'database_name',current_database())
FROM pg_control_system() s JOIN pg_database d ON d.datname=current_database()"""


def sql(statement):
    return subprocess.run(['docker', 'exec', '-i', CONTAINER, 'psql', '-U', 'neondojo', '-d', 'neondojo', '-v', 'ON_ERROR_STOP=1', '-At'], input=statement, text=True, check=True, capture_output=True).stdout


def request(path, body=None, content_type='application/json'):
    payload = None if body is None else (body if isinstance(body, bytes) else json.dumps(body).encode())
    req = urllib.request.Request(API+path, data=payload, headers={'Content-Type': content_type})
    try:
        with urllib.request.urlopen(req, timeout=180) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        text = error.read().decode(errors='replace')
        try:
            text = json.loads(text).get('error', text)
        except ValueError:
            pass
        return error.code, {'error': text}


def write_manifest(path, report):
    """Keep either the old complete manifest or the new complete manifest."""
    descriptor, temporary = tempfile.mkstemp(prefix='.'+path.name+'.', dir=path.parent)
    try:
        with os.fdopen(descriptor, 'w') as file:
            json.dump(report, file, ensure_ascii=False, indent=2)
            file.flush()
            os.fsync(file.fileno())
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def target_snapshot():
    return json.loads(sql("SELECT jsonb_build_object('identity',("+IDENTITY_SQL+"),'databases',(SELECT coalesce(jsonb_agg(id::text),'[]'::jsonb) FROM databases));"))


def verify_api_target():
    status, databases = request('/databases')
    if status != 200 or not isinstance(databases, list):
        raise SystemExit('Cannot verify API database target')
    snapshot = target_snapshot()
    api_ids = {str(uuid.UUID(item['id'])) for item in databases}
    if api_ids != set(snapshot['databases']):
        raise SystemExit('API and container database IDs differ; no staging data written')
    return snapshot['identity']


def require_backend_stopped():
    try:
        with urllib.request.urlopen(API+'/health', timeout=2):
            pass
    except urllib.error.URLError as error:
        if isinstance(error.reason, OSError) and error.reason.errno == errno.ECONNREFUSED:
            return
        raise SystemExit('Cannot prove backend is stopped: expected connection refused') from error
    except OSError as error:
        if error.errno == errno.ECONNREFUSED:
            return
        raise SystemExit('Cannot prove backend is stopped: expected connection refused') from error
    raise SystemExit('Stop the backend before activating demo databases')


def activation_data(report):
    try:
        ids = {kind: str(uuid.UUID(value)) for kind, value in report['databases'].items()}
        identity = report['database_identity']
        if (set(ids) != {'bgf', 'mat', 'xg'} or len(set(ids.values())) != 3
                or DEFAULT in ids.values()):
            raise ValueError('Expected three distinct staging databases, excluding the default')
        if (set(identity) != {'system_identifier', 'database_oid', 'database_name'}
                or not all(isinstance(value, str) and value for value in identity.values())
                or not identity['system_identifier'].isdigit() or not identity['database_oid'].isdigit()):
            raise ValueError('Missing or invalid cluster/database identity')
        expected = {}
        populated = set()
        for item in report['files']:
            if item['http_status'] not in (200, 201):
                continue
            kind = Path(item['path']).parts[0]
            if kind not in ids or not re.fullmatch('[0-9a-f]{64}', item['sha256']):
                raise ValueError('Invalid staged source or SHA256')
            match = item['result']['match']
            row = {'id': str(uuid.UUID(match['id'])), 'database_id': ids[kind], 'sha256': item['sha256']}
            if (match.get('database_id', ids[kind]) != ids[kind]
                    or (row['id'] in expected and expected[row['id']] != row)):
                raise ValueError('Inconsistent staged match database or digest')
            expected[row['id']] = row
            populated.add(kind)
        if populated != set(ids):
            raise ValueError('Each source folder must have at least one expected match')
        return ids, {'identity': identity, 'matches': list(expected.values())}
    except (KeyError, IndexError, TypeError, AttributeError, ValueError) as error:
        raise SystemExit('Unsafe demo manifest: '+str(error)) from error


def activation_statement(report):
    ids, expected = activation_data(report)
    payload = json.dumps(expected).replace("'", "''")
    allowed = ','.join("'"+value+"'" for value in ids.values())
    # All preflight checks run under the same locks as the destructive swap.
    # A post-commit manifest failure is safe to retry: the BGF staging ID is gone.
    return f"""BEGIN;
LOCK TABLE databases, matches IN EXCLUSIVE MODE;
DO $demo$
DECLARE expected jsonb := '{payload}'::jsonb;
BEGIN
IF ({IDENTITY_SQL}) IS DISTINCT FROM expected->'identity' THEN
    RAISE EXCEPTION 'Cluster/database identity does not match demo manifest';
END IF;
IF (SELECT count(*) FROM databases WHERE id IN ({allowed})) <> 3
   OR NOT EXISTS (SELECT 1 FROM databases WHERE id='{DEFAULT}') THEN
    RAISE EXCEPTION 'Staging or default database is missing; activation aborted';
END IF;
IF EXISTS (
    SELECT 1 FROM jsonb_to_recordset(expected->'matches') e(id uuid,database_id uuid,sha256 text)
    LEFT JOIN matches m ON m.id=e.id
    WHERE m.id IS NULL OR m.database_id IS DISTINCT FROM e.database_id
       OR m.sha256 IS DISTINCT FROM e.sha256
) OR EXISTS (
    SELECT 1 FROM matches m WHERE m.database_id IN ({allowed}) AND NOT EXISTS (
        SELECT 1 FROM jsonb_to_recordset(expected->'matches') e(id uuid) WHERE e.id=m.id
    )
) THEN
    RAISE EXCEPTION 'Staged match IDs, database scopes or SHA256 values changed';
END IF;
END $demo$;
DELETE FROM matches WHERE database_id NOT IN ({allowed});
DELETE FROM databases WHERE id NOT IN ({allowed},'{DEFAULT}') AND NOT is_system;
UPDATE matches SET database_id='{DEFAULT}' WHERE database_id='{ids['bgf']}';
DELETE FROM databases WHERE id='{ids['bgf']}';
UPDATE databases SET name='Demo BGF',description='Private test and demo data from games/bgf',version=version+1 WHERE id='{DEFAULT}';
UPDATE databases SET name='Demo MAT',version=version+1 WHERE id='{ids['mat']}';
UPDATE databases SET name='Demo XG',version=version+1 WHERE id='{ids['xg']}';
COMMIT;
"""


def safe_output(path):
    path = path.expanduser().resolve()
    if path.is_relative_to(REPO):
        raise SystemExit('Reports and backups must be outside the repository')
    path.mkdir(parents=True, exist_ok=True)
    return path


def stage(args):
    sources = {kind: sorted(path for path in (args.games/kind).rglob('*') if path.is_file() and path.suffix.lower()=='.'+kind) for kind in ['bgf', 'mat', 'xg']}
    if any(not paths for paths in sources.values()):
        raise SystemExit('Expected nonempty games/bgf, games/mat and games/xg')
    identity = verify_api_target()
    output = safe_output(args.output)
    stamp = datetime.now(ZoneInfo('Europe/Berlin')).strftime('%Y-%m-%d-%H%M%S')
    backup = output/f'pre-demo-{stamp}.dump'
    with backup.open('wb') as file:
        subprocess.run(['docker', 'exec', CONTAINER, 'pg_dump', '-U', 'neondojo', '-d', 'neondojo', '-Fc'], stdout=file, check=True)
    backup.chmod(0o600)
    with backup.open('rb') as file:
        subprocess.run(['docker', 'exec', '-i', CONTAINER, 'pg_restore', '--file=/dev/null'], stdin=file, check=True)
    report = {'created_at': stamp, 'backup': str(backup), 'sources': str(args.games.resolve()), 'database_identity': identity, 'databases': {}, 'files': [], 'activated': False}
    manifest = output/f'demo-import-{stamp}.json'
    def save():
        write_manifest(manifest, report)
    for kind, paths in sources.items():
        status, database = request('/databases', {'name': f'Demo {kind.upper()} · staging {stamp}', 'description': f'Private test and demo data from games/{kind}'})
        if status != 201:
            raise SystemExit(f'Cannot create staging database: {database}')
        report['databases'][kind] = str(uuid.UUID(database['id']))
        save()
        snapshot = target_snapshot()
        if snapshot['identity'] != identity or report['databases'][kind] not in snapshot['databases']:
            raise SystemExit('Created API staging database is absent from the verified container database')
        for path in paths:
            data = path.read_bytes()
            boundary = 'neondojo-demo-'+uuid.uuid4().hex
            filename = path.name.replace('"', '_').replace('\r', '_').replace('\n', '_')
            body = f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{filename}"\r\nContent-Type: application/octet-stream\r\n\r\n'.encode()+data+f'\r\n--{boundary}--\r\n'.encode()
            status, result = request('/imports?database_id='+database['id'], body, 'multipart/form-data; boundary='+boundary)
            item = {'path': str(path.relative_to(args.games)), 'sha256': hashlib.sha256(data).hexdigest(), 'http_status': status, 'result': result}
            if status in (200, 201):
                analysis_status, summary = request('/matches/'+result['match']['id']+'/imported-analysis')
                if analysis_status != 200:
                    raise SystemExit(f'Analysis preservation endpoint failed: {summary}')
                item['imported_analysis'] = summary
            report['files'].append(item)
            save()
            print(f'{kind}/{path.name}: HTTP {status}', flush=True)
            if status not in (200, 201, 422):
                raise SystemExit('Unexpected import failure; old databases retained. See '+str(manifest))
    rejected = [item for item in report['files'] if item['http_status']==422]
    report['ready'] = not rejected or args.allow_rejected
    if report['ready']:
        activation_data(report)
    save()
    print(json.dumps({'manifest': str(manifest), 'backup': str(backup), 'imported': len(report['files'])-len(rejected), 'rejected': len(rejected), 'ready': report['ready']}), flush=True)
    if not report['ready']:
        raise SystemExit('Rejected files retained in report; pass --allow-rejected only after reviewing them')


def activate(args):
    manifest = args.manifest.expanduser().resolve()
    if manifest.is_relative_to(REPO):
        raise SystemExit('Manifest must be outside the repository')
    report = json.loads(manifest.read_text())
    if report.get('activated') or not report.get('ready'):
        raise SystemExit('Manifest is already active or not ready')
    if not Path(report['backup']).is_file():
        raise SystemExit('Backup is missing')
    require_backend_stopped()
    sql(activation_statement(report))
    report['databases']['bgf'] = DEFAULT
    report['activated'] = True
    report['activated_at'] = datetime.now(ZoneInfo('Europe/Berlin')).isoformat()
    write_manifest(manifest, report)
    print(json.dumps({'manifest': str(manifest), 'databases': report['databases'], 'activated': True}), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    staging = sub.add_parser('stage')
    staging.add_argument('--games', type=Path, default=REPO/'games')
    staging.add_argument('--output', type=Path, default=DATA/'demo')
    staging.add_argument('--allow-rejected', action='store_true')
    activation = sub.add_parser('activate')
    activation.add_argument('manifest', type=Path)
    args = parser.parse_args()
    try:
        (stage if args.action=='stage' else activate)(args)
    except (OSError, subprocess.CalledProcessError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)
