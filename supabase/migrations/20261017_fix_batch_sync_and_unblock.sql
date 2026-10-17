BEGIN;
SELECT pg_catalog.pg_advisory_xact_lock(71620261016);

-- Helper to extract all file paths regardless of whether files JSONB is ["a.cpp"] or [{"path":"a.cpp"}]
CREATE OR REPLACE FUNCTION public.all_file_paths(p_files jsonb)
RETURNS text[] LANGUAGE plpgsql IMMUTABLE SET search_path = '' AS $$
DECLARE
    all_paths text[];
BEGIN
    IF p_files IS NULL OR jsonb_typeof(p_files) <> 'array' THEN
        RETURN ARRAY[]::text[];
    END IF;
    SELECT coalesce(array_agg(val), ARRAY[]::text[]) INTO all_paths
    FROM (
        SELECT CASE WHEN jsonb_typeof(elem) = 'object' THEN elem->>'path'
                    ELSE elem #>> '{}' END AS val
        FROM jsonb_array_elements(p_files) elem
    ) sub WHERE coalesce(val, '') <> '';
    RETURN all_paths;
END $$;

GRANT EXECUTE ON FUNCTION public.all_file_paths(jsonb)
    TO anon, authenticated, service_role;

-- Replace available batches without brittle full-table JSON equality check
CREATE OR REPLACE FUNCTION public.replace_available_batches(
    p_pr_snapshot jsonb DEFAULT NULL,
    p_batch_snapshot jsonb DEFAULT NULL,
    p_batches jsonb DEFAULT '[]'::jsonb
)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE
    item jsonb;
    numbers integer[];
    b_type text;
BEGIN
    PERFORM pg_catalog.pg_advisory_xact_lock(71620261016);
    LOCK TABLE public.prs, public.batches IN SHARE ROW EXCLUSIVE MODE;

    IF p_batches IS NULL OR jsonb_typeof(p_batches) <> 'array' THEN
        RAISE EXCEPTION 'Invalid batches payload';
    END IF;

    -- 1. Delete all currently available (unclaimed) batches
    DELETE FROM public.batches
    WHERE status = 'available'
      AND claimed_user_id IS NULL
      AND coalesce(claimed_by, 'none') = 'none';

    -- 2. Clean up any orphan PR assignments not belonging to any remaining claimed batch
    UPDATE public.prs p
    SET assigned_user_id = NULL,
        assigned_reviewer = 'none',
        status = CASE WHEN p.status = 'in-review' THEN 'unassigned' ELSE p.status END,
        updated_at = now()
    WHERE (p.assigned_user_id IS NOT NULL OR coalesce(p.assigned_reviewer, 'none') <> 'none')
      AND NOT EXISTS (
          SELECT 1 FROM public.batches b
          WHERE b.status <> 'available'
            AND p.number = ANY(b.pr_numbers)
      );

    -- 3. Insert newly generated available batches
    FOR item IN SELECT value FROM jsonb_array_elements(p_batches)
    LOOP
        b_type := coalesce(item->>'batch_type', 'disjoint');
        IF b_type NOT IN ('disjoint', 'soft_disjoint', 'cohesive_cluster') THEN
            b_type := 'disjoint';
        END IF;

        SELECT ARRAY(
            SELECT DISTINCT (value::integer)
            FROM jsonb_array_elements_text(coalesce(item->'pr_numbers', '[]'::jsonb))
            ORDER BY 1
        ) INTO numbers;

        IF cardinality(numbers) = 0 THEN
            CONTINUE;
        END IF;

        -- Keep only PRs that are currently open, non-draft, unassigned, have files, and are not in another batch
        SELECT coalesce(array_agg(p.number ORDER BY p.number), ARRAY[]::integer[])
        INTO numbers
        FROM public.prs p
        WHERE p.number = ANY(numbers)
          AND p.github_state = 'open'
          AND NOT coalesce(p.is_draft, false)
          AND p.assigned_user_id IS NULL
          AND coalesce(p.assigned_reviewer, 'none') = 'none'
          AND jsonb_array_length(to_jsonb(p.files)) > 0
          AND NOT EXISTS (
              SELECT 1 FROM public.batches b
              WHERE b.pr_numbers && ARRAY[p.number]
          );

        IF cardinality(numbers) < 2 THEN
            CONTINUE;
        END IF;

        IF b_type = 'disjoint' THEN
            IF EXISTS (
                SELECT 1 FROM public.prs a
                JOIN public.prs b ON a.number < b.number
                  AND public.all_file_paths(to_jsonb(a.files)) && public.all_file_paths(to_jsonb(b.files))
                WHERE a.number = ANY(numbers) AND b.number = ANY(numbers)
            ) THEN
                -- Downgrade to soft_disjoint if only boilerplate overlaps, or cohesive_cluster otherwise
                IF NOT EXISTS (
                    SELECT 1 FROM public.prs a
                    JOIN public.prs b ON a.number < b.number
                      AND public.effective_code_files(to_jsonb(a.files)) && public.effective_code_files(to_jsonb(b.files))
                    WHERE a.number = ANY(numbers) AND b.number = ANY(numbers)
                ) THEN
                    b_type := 'soft_disjoint';
                ELSE
                    b_type := 'cohesive_cluster';
                END IF;
            END IF;
        ELSIF b_type = 'soft_disjoint' THEN
            IF EXISTS (
                SELECT 1 FROM public.prs a
                JOIN public.prs b ON a.number < b.number
                  AND public.effective_code_files(to_jsonb(a.files)) && public.effective_code_files(to_jsonb(b.files))
                WHERE a.number = ANY(numbers) AND b.number = ANY(numbers)
            ) THEN
                b_type := 'cohesive_cluster';
            END IF;
        END IF;

        INSERT INTO public.batches (
            id, title, category, complexity, complexity_label,
            pr_count, pr_numbers, status, claimed_by, batch_type
        )
        VALUES (
            item->>'id',
            public.batch_title_for_id(item->>'id'),
            coalesce(item->>'category', 'Other Core Subsystems'),
            coalesce(item->>'complexity', 'medium'),
            item->>'complexity_label',
            cardinality(numbers),
            numbers,
            'available',
            'none',
            b_type
        )
        ON CONFLICT (id) DO UPDATE SET
            title = EXCLUDED.title,
            category = EXCLUDED.category,
            complexity = EXCLUDED.complexity,
            complexity_label = EXCLUDED.complexity_label,
            pr_count = EXCLUDED.pr_count,
            pr_numbers = EXCLUDED.pr_numbers,
            batch_type = EXCLUDED.batch_type,
            updated_at = now()
        WHERE public.batches.status = 'available';
    END LOOP;
END $$;

REVOKE ALL ON FUNCTION public.replace_available_batches(jsonb, jsonb, jsonb) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.replace_available_batches(jsonb, jsonb, jsonb) TO service_role;

-- Direct Maintainer Claim Batch (via service_role + GitHub username)
CREATE OR REPLACE FUNCTION public.maintainer_claim_batch(p_batch_id text, p_username text DEFAULT 'maintainer')
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE
    batch public.batches%ROWTYPE;
    clean_user text := coalesce(nullif(trim(ltrim(coalesce(p_username, ''), '@')), ''), 'maintainer');
BEGIN
    PERFORM pg_catalog.pg_advisory_xact_lock(71620261016);
    SELECT * INTO batch FROM public.batches WHERE id = p_batch_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'Batch % not found', p_batch_id;
    END IF;
    IF batch.status <> 'available' THEN
        RAISE EXCEPTION 'Batch is already claimed by %', coalesce(batch.claimed_by, 'another maintainer');
    END IF;
    IF coalesce(cardinality(batch.pr_numbers), 0) = 0 THEN
        RAISE EXCEPTION 'Cannot claim an empty batch';
    END IF;

    UPDATE public.batches
    SET status = 'claimed',
        claimed_by = clean_user,
        claimed_user_id = NULL,
        claimed_at = now(),
        updated_at = now()
    WHERE id = p_batch_id;

    UPDATE public.prs
    SET assigned_reviewer = clean_user,
        assigned_user_id = NULL,
        status = 'in-review',
        updated_at = now()
    WHERE number = ANY(batch.pr_numbers);
END $$;

-- Direct Maintainer Release / Unblock Batch (releases any claimed or blocked batch)
CREATE OR REPLACE FUNCTION public.maintainer_release_batch(p_batch_id text)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE
    batch public.batches%ROWTYPE;
    open_valid_count integer := 0;
BEGIN
    PERFORM pg_catalog.pg_advisory_xact_lock(71620261016);
    SELECT * INTO batch FROM public.batches WHERE id = p_batch_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'Batch % not found', p_batch_id;
    END IF;

    -- Free all PRs in this batch
    UPDATE public.prs
    SET assigned_reviewer = 'none',
        assigned_user_id = NULL,
        status = CASE
            WHEN github_state = 'merged' THEN 'merged'
            WHEN github_state = 'closed' THEN 'closed'
            ELSE 'unassigned'
        END,
        updated_at = now()
    WHERE number = ANY(batch.pr_numbers);

    -- Check if all PRs in the batch are still open and non-draft
    SELECT count(*) INTO open_valid_count
    FROM public.prs
    WHERE number = ANY(batch.pr_numbers)
      AND github_state = 'open'
      AND NOT coalesce(is_draft, false);

    IF open_valid_count = cardinality(batch.pr_numbers) AND open_valid_count > 0 THEN
        UPDATE public.batches
        SET status = 'available',
            claimed_by = 'none',
            claimed_user_id = NULL,
            claimed_at = NULL,
            updated_at = now()
        WHERE id = p_batch_id;
        RETURN jsonb_build_object('action', 'released', 'batch_id', p_batch_id);
    ELSE
        -- Batch had closed/merged/draft PRs blocking it: remove the stale batch record so open PRs can be re-batched cleanly
        DELETE FROM public.batches WHERE id = p_batch_id;
        RETURN jsonb_build_object('action', 'dissolved_blocked', 'batch_id', p_batch_id, 'open_prs_freed', open_valid_count);
    END IF;
END $$;

-- Direct Maintainer Complete Batch
CREATE OR REPLACE FUNCTION public.maintainer_complete_batch(p_batch_id text)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE
    batch public.batches%ROWTYPE;
BEGIN
    PERFORM pg_catalog.pg_advisory_xact_lock(71620261016);
    SELECT * INTO batch FROM public.batches WHERE id = p_batch_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'Batch % not found', p_batch_id;
    END IF;

    UPDATE public.prs
    SET status = CASE
            WHEN github_state = 'merged' THEN 'merged'
            WHEN github_state = 'closed' THEN 'closed'
            ELSE 'unassigned'
        END,
        assigned_reviewer = CASE
            WHEN github_state IN ('merged', 'closed') THEN assigned_reviewer
            ELSE 'none'
        END,
        assigned_user_id = NULL,
        updated_at = now()
    WHERE number = ANY(batch.pr_numbers);

    DELETE FROM public.batches WHERE id = p_batch_id;
END $$;

-- Direct Maintainer Release All Batches for User (or '*' for all claimed batches)
CREATE OR REPLACE FUNCTION public.maintainer_release_all_batches(p_username text DEFAULT '*')
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE
    b record;
    clean_user text := coalesce(nullif(trim(ltrim(coalesce(p_username, '*'), '@')), ''), '*');
    released_count integer := 0;
BEGIN
    FOR b IN
        SELECT id FROM public.batches
        WHERE status <> 'available'
          AND (clean_user = '*' OR lower(coalesce(claimed_by, '')) = lower(clean_user))
    LOOP
        PERFORM public.maintainer_release_batch(b.id);
        released_count := released_count + 1;
    END LOOP;
    RETURN jsonb_build_object('released_batches', released_count);
END $$;

-- Direct Maintainer Save PR Review
CREATE OR REPLACE FUNCTION public.maintainer_save_review(
    p_pr_number integer,
    p_status text,
    p_notes text,
    p_checklist jsonb,
    p_reviewer text DEFAULT 'maintainer'
)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE
    clean_user text := coalesce(nullif(trim(ltrim(coalesce(p_reviewer, ''), '@')), ''), 'maintainer');
BEGIN
    IF p_status NOT IN ('in-review', 'approved', 'changes', 'unassigned') THEN
        RAISE EXCEPTION 'Unsupported review status: %', p_status;
    END IF;

    UPDATE public.prs
    SET status = p_status,
        notes = coalesce(p_notes, ''),
        checklist = coalesce(p_checklist, '{}'::jsonb),
        assigned_reviewer = CASE
            WHEN coalesce(assigned_reviewer, 'none') = 'none' THEN clean_user
            ELSE assigned_reviewer
        END,
        updated_at = now()
    WHERE number = p_pr_number;
END $$;

REVOKE ALL ON FUNCTION
    public.maintainer_claim_batch(text, text),
    public.maintainer_release_batch(text),
    public.maintainer_release_all_batches(text),
    public.maintainer_complete_batch(text),
    public.maintainer_save_review(integer, text, text, jsonb, text)
FROM PUBLIC, anon, authenticated;

GRANT EXECUTE ON FUNCTION
    public.maintainer_claim_batch(text, text),
    public.maintainer_release_batch(text),
    public.maintainer_release_all_batches(text),
    public.maintainer_complete_batch(text),
    public.maintainer_save_review(integer, text, text, jsonb, text)
TO service_role;

NOTIFY pgrst, 'reload schema';
COMMIT;
