"""Apply only the Batch x naming migration; never run pending migrations."""
import hashlib
import json
import os
from datetime import datetime, timezone
from pathlib import Path
import pg8000.native


def apply():
    migration = Path('supabase/migrations/20261017_batch_names.sql')
    sql = migration.read_text(encoding='utf-8')
    checksum = hashlib.sha256(sql.encode()).hexdigest()
    conn = pg8000.native.Connection(
        user=os.environ['SUPABASE_DB_USER'], host=os.environ['SUPABASE_DB_HOST'],
        port=6543, database='postgres', password=os.environ['SUPABASE_DB_PASSWORD'],
        ssl_context=True, timeout=30)
    try:
        conn.run('BEGIN')
        conn.run("SET LOCAL lock_timeout = '10s'")
        conn.run('SELECT pg_advisory_xact_lock(71620261016)')
        conn.run('LOCK TABLE public.batches IN SHARE ROW EXCLUSIVE MODE')
        previous = conn.run('SELECT checksum FROM public.hub_schema_migrations WHERE name=:name', name=migration.name)
        if previous:
            if previous[0][0] != checksum:
                raise RuntimeError('Migration checksum mismatch')
            conn.run('ROLLBACK')
            print('Naming migration already applied.')
            return
        before = conn.run("SELECT coalesce(jsonb_agg(to_jsonb(b) ORDER BY id), '[]'::jsonb) FROM public.batches b")[0][0]
        folder = Path('.local-backups'); folder.mkdir(exist_ok=True)
        backup = folder / (datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ') + '-batch-names.json')
        backup.write_text(json.dumps(before, indent=2), encoding='utf-8')
        conn.run(sql.replace('BEGIN;', '', 1).rsplit('COMMIT;', 1)[0])
        after = conn.run("SELECT coalesce(jsonb_agg(to_jsonb(b) ORDER BY id), '[]'::jsonb) FROM public.batches b")[0][0]
        unchanged = lambda rows: [{k: v for k, v in row.items() if k != 'title'} for row in rows]
        if unchanged(before) != unchanged(after):
            raise RuntimeError('Unexpected changes outside batch titles; rolling back')
        names = [row['title'] for row in after]
        if len(set(names)) != len(names) or any(not title.startswith('Batch ') or not title[6:].isdigit() for title in names):
            raise RuntimeError('Invalid batch numbering; rolling back')
        conn.run('INSERT INTO public.hub_schema_migrations(name,checksum) VALUES (:name,:checksum)', name=migration.name, checksum=checksum)
        conn.run('COMMIT')
        print(f'Updated {len(after)} titles. All other batch fields verified unchanged. Backup saved.')
        print('Samples:', ', '.join(names[:3]))
    except Exception:
        conn.run('ROLLBACK')
        raise
    finally:
        conn.close()
