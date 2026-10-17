BEGIN;
SELECT pg_catalog.pg_advisory_xact_lock(71620261016);

-- Coordinator table for distributed multi-instance sync & heartbeat
CREATE TABLE IF NOT EXISTS public.sync_coordinator (
    id integer PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    data_version bigint NOT NULL DEFAULT 1,
    last_synced_at timestamptz NOT NULL DEFAULT now(),
    lease_owner text,
    lease_user_label text,
    lease_until timestamptz,
    active_instances jsonb NOT NULL DEFAULT '{}'::jsonb,
    updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE public.sync_coordinator ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON public.sync_coordinator FROM PUBLIC, anon, authenticated;
GRANT SELECT, INSERT, UPDATE ON public.sync_coordinator TO anon, authenticated, service_role;

-- Initialize single row if missing
INSERT INTO public.sync_coordinator (id, data_version, last_synced_at)
VALUES (1, 1, now())
ON CONFLICT (id) DO NOTHING;

-- Bumping data_version trigger function
CREATE OR REPLACE FUNCTION public.bump_sync_data_version()
RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
BEGIN
    UPDATE public.sync_coordinator
    SET data_version = data_version + 1,
        updated_at = now()
    WHERE id = 1;
    RETURN NULL;
END $$;

DROP TRIGGER IF EXISTS bump_data_version_on_prs ON public.prs;
CREATE TRIGGER bump_data_version_on_prs
AFTER INSERT OR UPDATE OR DELETE ON public.prs
FOR EACH STATEMENT EXECUTE FUNCTION public.bump_sync_data_version();

DROP TRIGGER IF EXISTS bump_data_version_on_batches ON public.batches;
CREATE TRIGGER bump_data_version_on_batches
AFTER INSERT OR UPDATE OR DELETE ON public.batches
FOR EACH STATEMENT EXECUTE FUNCTION public.bump_sync_data_version();

-- Heartbeat RPC: registers presence, purges instances older than 90s, returns cluster metadata
CREATE OR REPLACE FUNCTION public.sync_heartbeat(p_instance_id text, p_user_label text DEFAULT 'maintainer')
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE
    coord public.sync_coordinator%ROWTYPE;
    clean_instances jsonb := '{}'::jsonb;
    inst_key text;
    inst_val jsonb;
    now_ts timestamptz := clock_timestamp();
    sorted_keys text[];
    my_slot integer := 0;
    open_numbers integer[];
BEGIN
    PERFORM pg_catalog.pg_advisory_xact_lock(71620261017);
    SELECT * INTO coord FROM public.sync_coordinator WHERE id = 1 FOR UPDATE;
    IF NOT FOUND THEN
        INSERT INTO public.sync_coordinator (id, data_version, last_synced_at)
        VALUES (1, 1, now_ts) RETURNING * INTO coord;
    END IF;

    -- Clean stale instances (> 90 seconds old)
    FOR inst_key, inst_val IN SELECT * FROM jsonb_each(coord.active_instances)
    LOOP
        IF (inst_val->>'seen_at')::timestamptz > (now_ts - interval '90 seconds') THEN
            clean_instances := clean_instances || jsonb_build_object(inst_key, inst_val);
        END IF;
    END LOOP;

    -- Register / refresh caller instance
    IF p_instance_id IS NOT NULL AND length(trim(p_instance_id)) > 0 THEN
        clean_instances := clean_instances || jsonb_build_object(
            p_instance_id,
            jsonb_build_object('label', coalesce(p_user_label, 'maintainer'), 'seen_at', now_ts)
        );
    END IF;

    -- Clear expired lease if any
    IF coord.lease_until IS NOT NULL AND coord.lease_until <= now_ts THEN
        coord.lease_owner := NULL;
        coord.lease_user_label := NULL;
        coord.lease_until := NULL;
    END IF;

    UPDATE public.sync_coordinator
    SET active_instances = clean_instances,
        lease_owner = coord.lease_owner,
        lease_user_label = coord.lease_user_label,
        lease_until = coord.lease_until,
        updated_at = now_ts
    WHERE id = 1
    RETURNING * INTO coord;

    -- Calculate deterministic slot index for caller
    SELECT array_agg(k ORDER BY k) INTO sorted_keys FROM jsonb_object_keys(clean_instances) AS k;
    IF p_instance_id IS NOT NULL THEN
        FOR idx IN 1..coalesce(array_length(sorted_keys, 1), 0) LOOP
            IF sorted_keys[idx] = p_instance_id THEN
                my_slot := idx - 1;
                EXIT;
            END IF;
        END LOOP;
    END IF;

    -- Collect open PR numbers for client cache sync
    SELECT coalesce(array_agg(number ORDER BY number ASC), ARRAY[]::integer[])
    INTO open_numbers
    FROM public.prs WHERE github_state = 'open';

    RETURN jsonb_build_object(
        'data_version', coord.data_version,
        'last_synced_at', coord.last_synced_at,
        'lease_owner', coord.lease_owner,
        'lease_user_label', coord.lease_user_label,
        'lease_until', coord.lease_until,
        'active_count', coalesce(array_length(sorted_keys, 1), 1),
        'slot_index', my_slot,
        'open_pr_numbers', open_numbers
    );
END $$;

-- Try Acquire Lease RPC
CREATE OR REPLACE FUNCTION public.try_acquire_sync_lease(
    p_instance_id text,
    p_user_label text DEFAULT 'maintainer',
    p_min_interval_secs integer DEFAULT 180,
    p_ttl_secs integer DEFAULT 120
)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE
    coord public.sync_coordinator%ROWTYPE;
    now_ts timestamptz := clock_timestamp();
    has_lock boolean;
BEGIN
    SELECT pg_try_advisory_xact_lock(71620261017) INTO has_lock;
    IF NOT has_lock THEN
        RETURN jsonb_build_object('acquired', false, 'reason', 'busy_lock');
    END IF;

    SELECT * INTO coord FROM public.sync_coordinator WHERE id = 1 FOR UPDATE;

    -- If another instance currently holds an active lease
    IF coord.lease_until IS NOT NULL AND coord.lease_until > now_ts
       AND coord.lease_owner IS DISTINCT FROM p_instance_id THEN
        RETURN jsonb_build_object(
            'acquired', false,
            'reason', 'leased',
            'lease_owner', coord.lease_owner,
            'lease_user_label', coord.lease_user_label,
            'lease_until', coord.lease_until
        );
    END IF;

    -- If already synced recently and not forced (p_min_interval_secs > 0)
    IF p_min_interval_secs > 0
       AND coord.last_synced_at > (now_ts - (p_min_interval_secs || ' seconds')::interval)
       AND (coord.lease_owner IS NULL OR coord.lease_owner = p_instance_id) THEN
        RETURN jsonb_build_object(
            'acquired', false,
            'reason', 'fresh',
            'last_synced_at', coord.last_synced_at
        );
    END IF;

    -- Grant lease
    UPDATE public.sync_coordinator
    SET lease_owner = p_instance_id,
        lease_user_label = coalesce(p_user_label, 'maintainer'),
        lease_until = now_ts + (p_ttl_secs || ' seconds')::interval,
        updated_at = now_ts
    WHERE id = 1;

    RETURN jsonb_build_object(
        'acquired', true,
        'lease_until', now_ts + (p_ttl_secs || ' seconds')::interval
    );
END $$;

-- Release / Finish Lease RPC
CREATE OR REPLACE FUNCTION public.finish_sync_lease(p_instance_id text)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE
    now_ts timestamptz := clock_timestamp();
BEGIN
    PERFORM pg_catalog.pg_advisory_xact_lock(71620261017);
    UPDATE public.sync_coordinator
    SET lease_owner = NULL,
        lease_user_label = NULL,
        lease_until = NULL,
        last_synced_at = now_ts,
        data_version = data_version + 1,
        updated_at = now_ts
    WHERE id = 1 AND (lease_owner = p_instance_id OR lease_owner IS NULL);

    RETURN jsonb_build_object('success', true);
END $$;

GRANT EXECUTE ON FUNCTION public.sync_heartbeat(text, text),
                         public.try_acquire_sync_lease(text, text, integer, integer),
                         public.finish_sync_lease(text)
    TO anon, authenticated, service_role;

NOTIFY pgrst, 'reload schema';
COMMIT;
