"""Check concurrent claims in disposable schemas, without touching production rows."""
import os
from pathlib import Path
import threading
import uuid
import pg8000.native


def connect():
    return pg8000.native.Connection(
        user=os.environ['SUPABASE_DB_USER'], host=os.environ['SUPABASE_DB_HOST'],
        port=int(os.environ.get('SUPABASE_DB_PORT', '6543')), database='postgres',
        password=os.environ['SUPABASE_DB_PASSWORD'], ssl_context=True, timeout=30)


def check():
    schema = 'hub_check_' + uuid.uuid4().hex
    auth_schema = schema + '_auth'
    admin = connect()
    users = [str(uuid.uuid4()), str(uuid.uuid4())]
    try:
        admin.run(f'CREATE SCHEMA {schema}; CREATE SCHEMA {auth_schema}')
        admin.run(f'GRANT USAGE ON SCHEMA {schema}, {auth_schema} TO anon, authenticated, service_role')
        for table in ('prs', 'batches'):
            admin.run(f'CREATE TABLE {schema}.{table} (LIKE public.{table} INCLUDING ALL)')
        admin.run(f'ALTER TABLE {schema}.prs DROP COLUMN IF EXISTS assigned_user_id, '
                  'DROP COLUMN IF EXISTS github_state, DROP COLUMN IF EXISTS head_sha')
        admin.run(f'ALTER TABLE {schema}.batches DROP COLUMN IF EXISTS claimed_user_id')
        admin.run(f'CREATE TABLE {auth_schema}.users(id uuid PRIMARY KEY)')
        admin.run(f'CREATE TABLE {auth_schema}.identities(user_id uuid,provider text,identity_data jsonb)')
        admin.run(f"CREATE FUNCTION {auth_schema}.uid() RETURNS uuid LANGUAGE sql AS "
                  "$$ SELECT nullif(current_setting('request.jwt.claim.sub',true),'')::uuid $$")
        for path in sorted(Path('supabase/migrations').glob('*.sql')):
            sql = path.read_text(encoding='utf-8-sig').replace('BEGIN;', '', 1).rsplit('COMMIT;', 1)[0]
            sql = sql.replace('public.', schema + '.').replace('auth.', auth_schema + '.')
            sql = sql.replace("schemaname = 'public'", f"schemaname = '{schema}'")
            admin.run(sql)
        for index, user in enumerate(users):
            admin.run(f"INSERT INTO {auth_schema}.users VALUES ('{user}')")
            admin.run(f"INSERT INTO {auth_schema}.identities VALUES ('{user}','github',"
                      f"'{{\"user_name\":\"test-{index}\"}}')")
            admin.run(f"INSERT INTO {schema}.hub_maintainers VALUES ('{user}')")
        admin.run(f"INSERT INTO {schema}.prs(number,title,author,files,github_state) "
                  "VALUES (1,'test','fixture','[\"shared.cpp\"]','open')")
        for batch in ('a', 'b'):
            admin.run(f"INSERT INTO {schema}.batches(id,title,category,complexity,complexity_label,pr_count,pr_numbers) "
                      f"VALUES ('{batch}','test','test','low','low',1,ARRAY[1])")

        def race(batch_ids):
            barrier = threading.Barrier(2)
            results = []
            def claim(user, batch):
                conn = connect()
                try:
                    conn.run('BEGIN')
                    conn.run("SET LOCAL statement_timeout = '10s'")
                    conn.run(f"SELECT set_config('request.jwt.claim.sub','{user}',true)")
                    conn.run('SET LOCAL ROLE authenticated')
                    barrier.wait(timeout=10)
                    conn.run(f"SELECT {schema}.claim_batch('{batch}')")
                    conn.run('COMMIT')
                    results.append('claimed')
                except Exception:
                    conn.run('ROLLBACK')
                    results.append('rejected')
                finally:
                    conn.close()
            workers = [threading.Thread(target=claim, args=pair)
                       for pair in zip(users, batch_ids)]
            for worker in workers:
                worker.start()
            for worker in workers:
                worker.join(timeout=30)
            assert sorted(results) == ['claimed', 'rejected'], results

        race(['a', 'a'])
        # Reset only disposable fixtures, then race two overlapping batches.
        admin.run(f"UPDATE {schema}.batches SET status='available',claimed_by='none',claimed_user_id=NULL")
        admin.run(f"UPDATE {schema}.prs SET assigned_reviewer='none',assigned_user_id=NULL")
        race(['a', 'b'])
        print('PASS: concurrent claims of one batch and overlapping batches have exactly one winner')
    finally:
        admin.run(f'DROP SCHEMA IF EXISTS {schema} CASCADE; DROP SCHEMA IF EXISTS {auth_schema} CASCADE')
        admin.close()


if __name__ == '__main__':
    check()
