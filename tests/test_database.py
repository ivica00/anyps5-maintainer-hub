"""Opt-in rollback-only integration check; never commits fixtures or migrations."""
import json
import os
from pathlib import Path
import unittest


@unittest.skipUnless(os.environ.get('RUN_SUPABASE_ROLLBACK_TESTS') == '1',
                     'Set RUN_SUPABASE_ROLLBACK_TESTS=1 for database validation')
class DatabaseTests(unittest.TestCase):
    def test_migrations_permissions_and_atomic_claims(self):
        import pg8000.native
        conn = pg8000.native.Connection(
            user=os.environ['SUPABASE_DB_USER'], host=os.environ['SUPABASE_DB_HOST'],
            port=int(os.environ.get('SUPABASE_DB_PORT', '6543')), database='postgres',
            password=os.environ['SUPABASE_DB_PASSWORD'], ssl_context=True, timeout=30)
        owner = '1386c497-d737-4ab8-9af8-92ee68679901'
        stranger = '1386c497-d737-4ab8-9af8-92ee68679902'
        try:
            conn.run('BEGIN')
            conn.run("SET LOCAL lock_timeout = '5s'")
            tracking = conn.run("SELECT to_regclass('public.hub_schema_migrations')")[0][0]
            applied = {row[0] for row in conn.run('SELECT name FROM public.hub_schema_migrations')} if tracking else set()
            for path in sorted(Path('supabase/migrations').glob('*.sql')):
                if path.name not in applied:
                    sql = path.read_text(encoding='utf-8-sig').replace('BEGIN;', '', 1)
                    conn.run(sql.rsplit('COMMIT;', 1)[0])
            conn.run(f"INSERT INTO auth.users(id) VALUES ('{owner}'), ('{stranger}')")
            conn.run(f"INSERT INTO auth.identities(user_id,provider,provider_id,identity_data) "
                     f"VALUES ('{owner}','github','rollback-test-owner',"
                     "'{\"user_name\":\"rollback-test-owner\"}')")
            conn.run(f"INSERT INTO public.hub_maintainers VALUES ('{owner}')")
            conn.run("INSERT INTO public.prs(number,title,author,files,github_state,head_sha,notes) VALUES "
                     "(-100001,'rollback fixture','fixture','[\"rollback-unique-file.cpp\"]','open','sha1','keep notes')")
            conn.run("INSERT INTO public.batches(id,title,category,complexity,complexity_label,pr_count,pr_numbers) "
                     "VALUES ('rollback-fixture','test','test','low','low',1,ARRAY[-100001])")

            def reject(statement):
                conn.run('SAVEPOINT expected_denial')
                try:
                    conn.run(statement)
                except Exception:
                    conn.run('ROLLBACK TO SAVEPOINT expected_denial')
                else:
                    self.fail('Expected database rejection')
                finally:
                    conn.run('RELEASE SAVEPOINT expected_denial')

            conn.run('SET LOCAL ROLE anon')
            reject("UPDATE public.prs SET notes='bad' WHERE number=-100001")
            reject("SELECT public.claim_batch('rollback-fixture')")
            conn.run('RESET ROLE')
            conn.run(f"SELECT set_config('request.jwt.claim.sub','{stranger}',true)")
            conn.run('SET LOCAL ROLE authenticated')
            reject("SELECT public.claim_batch('rollback-fixture')")
            conn.run('RESET ROLE')
            conn.run(f"SELECT set_config('request.jwt.claim.sub','{owner}',true)")
            conn.run('SET LOCAL ROLE authenticated')
            conn.run("SELECT public.claim_batch('rollback-fixture')")
            reject("SELECT public.claim_batch('rollback-fixture')")
            conn.run("UPDATE public.prs SET notes='keep updated notes' WHERE number=-100001")
            reject("UPDATE public.prs SET assigned_reviewer='other' WHERE number=-100001")
            conn.run('RESET ROLE')
            conn.run(f"SELECT set_config('request.jwt.claim.sub','{stranger}',true)")
            conn.run('SET LOCAL ROLE authenticated')
            reject("SELECT public.release_batch('rollback-fixture')")
            conn.run('RESET ROLE')
            # Regeneration replaces available batches, while claimed membership is immutable.
            pr_snapshot = conn.run("SELECT coalesce(jsonb_agg(to_jsonb(p)), '[]'::jsonb) FROM public.prs p")[0][0]
            batch_snapshot = conn.run("SELECT coalesce(jsonb_agg(to_jsonb(b)), '[]'::jsonb) FROM public.batches b")[0][0]
            conn.run("SELECT public.replace_available_batches(CAST(:prs AS jsonb), CAST(:batches AS jsonb), '[]')",
                     prs=json.dumps(pr_snapshot), batches=json.dumps(batch_snapshot))
            self.assertEqual(conn.run("SELECT pr_numbers,claimed_user_id::text FROM public.batches WHERE id='rollback-fixture'")[0],
                             [[-100001], owner])
            self.assertEqual(conn.run("SELECT count(*) FROM public.batches WHERE status='available' AND coalesce(claimed_by,'none')='none'")[0][0], 0)
            record = {'number': -100001, 'title': 'new title', 'author': 'fixture', 'files': ['rollback-unique-file.cpp'],
                      'github_state': 'open', 'head_sha': 'sha2'}
            conn.run('SELECT public.sync_github_snapshot(CAST(:records AS jsonb), CAST(:states AS jsonb))',
                     records=json.dumps([record]),
                     states=json.dumps([{'number': -100001, 'github_state': 'merged'}]))
            row = conn.run('SELECT notes,assigned_user_id::text,github_state,status FROM public.prs WHERE number=-100001')[0]
            self.assertEqual(row, ['keep updated notes', owner, 'merged', 'in-review'])
            # A merged PR's historical assignment must not reserve its files forever.
            conn.run("INSERT INTO public.prs(number,title,author,files,github_state) VALUES "
                     "(-100002,'new fixture','fixture','[\"rollback-unique-file.cpp\"]','open')")
            conn.run("INSERT INTO public.batches(id,title,category,complexity,complexity_label,pr_count,pr_numbers) "
                     "VALUES ('rollback-second','test','test','low','low',1,ARRAY[-100002])")
            conn.run(f"SELECT set_config('request.jwt.claim.sub','{owner}',true)")
            conn.run('SET LOCAL ROLE authenticated')
            conn.run("SELECT public.claim_batch('rollback-second')")
            conn.run("SELECT public.release_batch('rollback-second')")
            conn.run("SELECT public.release_batch('rollback-fixture')")
            reject("SELECT public.claim_batch('rollback-fixture')")
            conn.run('RESET ROLE')
            self.assertEqual(conn.run("SELECT status FROM public.batches WHERE id='rollback-fixture'")[0][0], 'available')
            reject("SELECT public.replace_available_batches('[]','[]','[]')")
        finally:
            conn.run('ROLLBACK')
            conn.close()


if __name__ == '__main__':
    unittest.main()
