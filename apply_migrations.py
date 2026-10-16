"""Back up hub tables, then apply checksum-tracked migrations atomically."""
import hashlib
import json
import os
from datetime import datetime, timezone
from pathlib import Path
import pg8000.native


def apply():
    conn = pg8000.native.Connection(
        user=os.environ['SUPABASE_DB_USER'], host=os.environ['SUPABASE_DB_HOST'],
        port=int(os.environ.get('SUPABASE_DB_PORT', '6543')), database='postgres',
        password=os.environ['SUPABASE_DB_PASSWORD'], ssl_context=True, timeout=120)
    try:
        conn.run('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY')
        backup = {}
        for table in ('prs', 'batches'):
            backup[table] = conn.run(f"SELECT coalesce(jsonb_agg(to_jsonb(t)), '[]'::jsonb) FROM public.{table} t")[0][0]
        backup['policies'] = conn.run("SELECT coalesce(jsonb_agg(to_jsonb(p)), '[]'::jsonb) FROM pg_policies p "
                                      "WHERE schemaname='public' AND tablename IN ('prs','batches')")[0][0]
        backup['columns'] = conn.run("SELECT coalesce(jsonb_agg(to_jsonb(c)), '[]'::jsonb) FROM information_schema.columns c "
                                      "WHERE table_schema='public' AND table_name IN ('prs','batches')")[0][0]
        backup['grants'] = conn.run("SELECT coalesce(jsonb_agg(to_jsonb(g)), '[]'::jsonb) FROM information_schema.role_table_grants g "
                                     "WHERE table_schema='public' AND table_name IN ('prs','batches')")[0][0]
        conn.run('COMMIT')
        folder = Path('.local-backups')
        folder.mkdir(exist_ok=True)
        path = folder / (datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ') + '.json')
        path.write_text(json.dumps(backup, ensure_ascii=False, indent=2), encoding='utf-8')
        print(f'Backup saved: {len(backup["prs"])} PRs, {len(backup["batches"])} batches', flush=True)

        conn.run('BEGIN')
        conn.run("SET LOCAL lock_timeout = '10s'")
        conn.run('SELECT pg_advisory_xact_lock(71620261016)')
        conn.run('CREATE TABLE IF NOT EXISTS public.hub_schema_migrations '
                 '(name text PRIMARY KEY, checksum text NOT NULL, applied_at timestamptz NOT NULL DEFAULT now())')
        conn.run('REVOKE ALL ON public.hub_schema_migrations FROM PUBLIC, anon, authenticated')
        conn.run('ALTER TABLE public.hub_schema_migrations ENABLE ROW LEVEL SECURITY')
        for migration in sorted(Path('supabase/migrations').glob('*.sql')):
            sql = migration.read_text(encoding='utf-8-sig')
            checksum = hashlib.sha256(sql.encode()).hexdigest()
            prior = conn.run('SELECT checksum FROM public.hub_schema_migrations WHERE name=:name', name=migration.name)
            if prior:
                if prior[0][0] != checksum:
                    raise RuntimeError('An applied migration was modified: ' + migration.name)
                continue
            conn.run(sql.replace('BEGIN;', '', 1).rsplit('COMMIT;', 1)[0])
            conn.run('INSERT INTO public.hub_schema_migrations(name,checksum) VALUES (:name,:checksum)',
                     name=migration.name, checksum=checksum)
            print('Applied ' + migration.name, flush=True)
        conn.run('COMMIT')
    except Exception:
        conn.run('ROLLBACK')
        raise
    finally:
        conn.close()


if __name__ == '__main__':
    apply()
