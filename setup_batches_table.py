import os
import pg8000.native

conn = pg8000.native.Connection(
    user='postgres.nzaxslgwsygpgwqcllpi',
    host='aws-0-eu-west-1.pooler.supabase.com',
    port=6543,
    database='postgres',
    password=os.environ["SUPABASE_DB_PASSWORD"],
    ssl_context=True
)

statements = [
    """
    CREATE TABLE IF NOT EXISTS public.batches (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        category TEXT NOT NULL,
        complexity TEXT NOT NULL,
        complexity_label TEXT NOT NULL,
        pr_count INT NOT NULL,
        pr_numbers INT[] NOT NULL,
        status TEXT DEFAULT 'available',
        claimed_by TEXT DEFAULT 'none',
        claimed_at TIMESTAMPTZ,
        created_at TIMESTAMPTZ DEFAULT now(),
        updated_at TIMESTAMPTZ DEFAULT now()
    );
    """,
    "ALTER TABLE public.batches ENABLE ROW LEVEL SECURITY;",
    """
    DO $$
    BEGIN
        IF NOT EXISTS (
            SELECT 1 FROM pg_publication_tables 
            WHERE pubname = 'supabase_realtime' AND tablename = 'batches'
        ) THEN
            ALTER PUBLICATION supabase_realtime ADD TABLE public.batches;
        END IF;
    END $$;
    """,
    "NOTIFY pgrst, 'reload schema';"
]

for s in statements:
    s = s.strip()
    if s:
        conn.run(s)

print("Bootstrap only: apply supabase/migrations/20261016_secure_batches.sql before use. Successfully created public.batches table with RLS and Realtime in Supabase!")
conn.close()
