BEGIN;
SELECT pg_catalog.pg_advisory_xact_lock(71620261016);
LOCK TABLE public.prs, public.batches IN SHARE ROW EXCLUSIVE MODE;

ALTER TABLE public.batches ADD COLUMN IF NOT EXISTS batch_type text NOT NULL DEFAULT 'disjoint';
ALTER TABLE public.batches DROP CONSTRAINT IF EXISTS batches_batch_type_valid;
ALTER TABLE public.batches ADD CONSTRAINT batches_batch_type_valid
    CHECK (batch_type IN ('disjoint', 'soft_disjoint', 'cohesive_cluster'));

ALTER TABLE public.prs ADD COLUMN IF NOT EXISTS mergeable_state text NOT NULL DEFAULT 'unknown';

CREATE OR REPLACE FUNCTION public.check_claimable_batch()
RETURNS trigger LANGUAGE plpgsql SET search_path = '' AS $$
BEGIN
    IF NEW.status = 'claimed' AND (
        (OLD.status IS DISTINCT FROM NEW.status AND OLD.claimed_user_id IS NULL)
        OR OLD.pr_numbers IS DISTINCT FROM NEW.pr_numbers
    ) THEN
        IF EXISTS (SELECT 1 FROM public.prs WHERE number = ANY(NEW.pr_numbers)
            AND (github_state <> 'open' OR coalesce(is_draft, false)
                 OR coalesce(jsonb_array_length(to_jsonb(files)), 0) = 0)) THEN
            RAISE EXCEPTION 'Batch contains closed, draft or unverified PRs';
        END IF;
    END IF;
    RETURN NEW;
END $$;

-- Reconcile any claimed batch whose status was left as 'available' while claimed_user_id remained set.
UPDATE public.batches b
SET status = 'claimed',
    claimed_by = coalesce(
        nullif(b.claimed_by, 'none'),
        (SELECT coalesce(identity_data->>'user_name', identity_data->>'preferred_username')
         FROM auth.identities i WHERE i.user_id = b.claimed_user_id AND i.provider = 'github' LIMIT 1),
        'ivica00'
    )
WHERE b.claimed_user_id IS NOT NULL AND b.status = 'available';

ALTER TABLE public.batches DROP CONSTRAINT IF EXISTS batches_availability_consistent;
ALTER TABLE public.batches ADD CONSTRAINT batches_availability_consistent
    CHECK ((status = 'available' AND claimed_user_id IS NULL AND coalesce(claimed_by, 'none') = 'none')
           OR status <> 'available');

CREATE OR REPLACE FUNCTION public.is_boilerplate_path(p_path text)
RETURNS boolean LANGUAGE sql IMMUTABLE SET search_path = '' AS $$
    SELECT lower(replace(coalesce(p_path, ''), '\', '/')) LIKE '%cmakelists.txt'
        OR lower(replace(coalesce(p_path, ''), '\', '/')) LIKE 'docs/dev/debt/%';
$$;

CREATE OR REPLACE FUNCTION public.effective_code_files(p_files jsonb)
RETURNS text[] LANGUAGE plpgsql IMMUTABLE SET search_path = '' AS $$
DECLARE
    all_paths text[];
    code_paths text[];
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

    SELECT coalesce(array_agg(p), ARRAY[]::text[]) INTO code_paths
    FROM unnest(all_paths) p WHERE NOT public.is_boilerplate_path(p);

    IF cardinality(code_paths) > 0 THEN
        RETURN code_paths;
    END IF;
    RETURN all_paths;
END $$;

GRANT EXECUTE ON FUNCTION public.is_boilerplate_path(text), public.effective_code_files(jsonb)
    TO anon, authenticated, service_role;

CREATE OR REPLACE FUNCTION public.sync_github_snapshot(p_records jsonb, p_states jsonb)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
BEGIN
    PERFORM pg_catalog.pg_advisory_xact_lock(71620261016);
    LOCK TABLE public.prs, public.batches IN SHARE ROW EXCLUSIVE MODE;
    IF jsonb_typeof(p_records) <> 'array' OR jsonb_typeof(p_states) <> 'array' THEN
        RAISE EXCEPTION 'Invalid GitHub snapshot';
    END IF;
    IF EXISTS (SELECT 1 FROM jsonb_array_elements(p_states) s
        WHERE s->>'github_state' NOT IN ('open','closed','merged')) THEN
        RAISE EXCEPTION 'Invalid GitHub state';
    END IF;
    INSERT INTO public.prs (
        number, title, author, author_avatar, created_at, url,
        additions, deletions, total_lines, file_count, files,
        category, hotspots, risk_level, size, is_draft,
        head_sha, github_state, mergeable_state
    )
    SELECT
        r.number, r.title, r.author, r.author_avatar, r.created_at, r.url,
        r.additions, r.deletions, r.total_lines, r.file_count, r.files,
        r.category, r.hotspots, r.risk_level, r.size, r.is_draft,
        r.head_sha, r.github_state, coalesce(r.mergeable_state, 'unknown')
    FROM jsonb_populate_recordset(NULL::public.prs, p_records) r
    ON CONFLICT (number) DO UPDATE SET
        title = EXCLUDED.title,
        author = EXCLUDED.author,
        author_avatar = EXCLUDED.author_avatar,
        created_at = EXCLUDED.created_at,
        url = EXCLUDED.url,
        additions = EXCLUDED.additions,
        deletions = EXCLUDED.deletions,
        total_lines = EXCLUDED.total_lines,
        file_count = EXCLUDED.file_count,
        files = EXCLUDED.files,
        category = EXCLUDED.category,
        hotspots = EXCLUDED.hotspots,
        risk_level = EXCLUDED.risk_level,
        size = EXCLUDED.size,
        is_draft = EXCLUDED.is_draft,
        head_sha = EXCLUDED.head_sha,
        github_state = EXCLUDED.github_state,
        mergeable_state = coalesce(EXCLUDED.mergeable_state, public.prs.mergeable_state, 'unknown'),
        checklist = CASE WHEN public.prs.head_sha IS DISTINCT FROM EXCLUDED.head_sha
                         THEN '{}'::jsonb ELSE public.prs.checklist END,
        status = CASE WHEN public.prs.head_sha IS DISTINCT FROM EXCLUDED.head_sha
                      AND public.prs.assigned_reviewer IS NOT NULL
                      AND public.prs.assigned_reviewer <> 'none'
                      THEN 'in-review' ELSE public.prs.status END,
        updated_at = now();
    UPDATE public.prs p SET github_state = s.github_state, updated_at = now()
        FROM jsonb_to_recordset(p_states) AS s(number integer, github_state text)
        WHERE p.number = s.number;
END $$;
REVOKE ALL ON FUNCTION public.sync_github_snapshot(jsonb,jsonb) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.sync_github_snapshot(jsonb,jsonb) TO service_role;

CREATE OR REPLACE FUNCTION public.replace_available_batches(
    p_pr_snapshot jsonb, p_batch_snapshot jsonb, p_batches jsonb)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE
    current_prs jsonb;
    current_batches jsonb;
    item jsonb;
    numbers integer[];
    b_type text;
BEGIN
    PERFORM pg_catalog.pg_advisory_xact_lock(71620261016);
    LOCK TABLE public.prs, public.batches IN SHARE ROW EXCLUSIVE MODE;
    SELECT coalesce(jsonb_agg(to_jsonb(p)), '[]'::jsonb) INTO current_prs FROM public.prs p;
    SELECT coalesce(jsonb_agg(to_jsonb(b)), '[]'::jsonb) INTO current_batches FROM public.batches b;
    IF NOT (current_prs @> p_pr_snapshot AND p_pr_snapshot @> current_prs
        AND current_batches @> p_batch_snapshot AND p_batch_snapshot @> current_batches)
    THEN RAISE EXCEPTION 'Data changed during generation; retry with a fresh snapshot'; END IF;
    IF jsonb_typeof(p_batches) <> 'array' THEN RAISE EXCEPTION 'Invalid batches'; END IF;

    -- Claimed and legacy-owned rows survive byte-for-byte, including their membership.
    DELETE FROM public.batches WHERE status = 'available' AND claimed_user_id IS NULL
        AND coalesce(claimed_by, 'none') = 'none';

    FOR item IN SELECT value FROM jsonb_array_elements(p_batches)
    LOOP
        b_type := coalesce(item->>'batch_type', 'disjoint');
        IF b_type NOT IN ('disjoint', 'soft_disjoint', 'cohesive_cluster') THEN
            RAISE EXCEPTION 'Invalid batch_type';
        END IF;

        SELECT ARRAY(SELECT value::integer FROM jsonb_array_elements_text(item->'pr_numbers')) INTO numbers;
        IF cardinality(numbers) = 0 OR cardinality(numbers) <> (item->>'pr_count')::integer
           OR cardinality(numbers) <> (SELECT count(DISTINCT n) FROM unnest(numbers) n)
        THEN RAISE EXCEPTION 'Invalid batch membership'; END IF;

        IF (SELECT count(*) FROM public.prs WHERE number = ANY(numbers) AND github_state = 'open'
            AND NOT coalesce(is_draft, false) AND assigned_user_id IS NULL
            AND coalesce(assigned_reviewer, 'none') = 'none'
            AND jsonb_array_length(to_jsonb(files)) > 0) <> cardinality(numbers)
        THEN RAISE EXCEPTION 'Batch contains unavailable PRs'; END IF;

        IF EXISTS (SELECT 1 FROM public.batches WHERE pr_numbers && numbers)
        THEN RAISE EXCEPTION 'PR already belongs to another batch'; END IF;

        IF b_type = 'disjoint' THEN
            IF EXISTS (SELECT 1 FROM public.prs a JOIN public.prs b ON a.number < b.number
                AND to_jsonb(a.files) ?| ARRAY(SELECT jsonb_array_elements_text(to_jsonb(b.files)))
                WHERE a.number = ANY(numbers) AND b.number = ANY(numbers))
            THEN RAISE EXCEPTION 'Batch contains overlapping files'; END IF;
        ELSIF b_type = 'soft_disjoint' THEN
            IF EXISTS (SELECT 1 FROM public.prs a JOIN public.prs b ON a.number < b.number
                AND public.effective_code_files(to_jsonb(a.files)) && public.effective_code_files(to_jsonb(b.files))
                WHERE a.number = ANY(numbers) AND b.number = ANY(numbers))
            THEN RAISE EXCEPTION 'Soft-disjoint batch contains overlapping code files'; END IF;
        END IF;

        INSERT INTO public.batches (
            id, title, category, complexity, complexity_label,
            pr_count, pr_numbers, status, claimed_by, batch_type
        )
        VALUES (
            item->>'id', public.batch_title_for_id(item->>'id'), item->>'category', item->>'complexity',
            item->>'complexity_label', cardinality(numbers), numbers, 'available', 'none', b_type
        );
    END LOOP;
END $$;
REVOKE ALL ON FUNCTION public.replace_available_batches(jsonb,jsonb,jsonb) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.replace_available_batches(jsonb,jsonb,jsonb) TO service_role;

CREATE OR REPLACE FUNCTION public.claim_batch(p_batch_id text)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE batch public.batches%ROWTYPE; username text; found_count integer;
BEGIN
    IF auth.uid() IS NULL OR NOT EXISTS (
        SELECT 1 FROM public.hub_maintainers WHERE user_id = auth.uid()
    ) THEN RAISE EXCEPTION 'Maintainer access required'; END IF;
    -- Serialize claim/release decisions, including overlapping batches.
    PERFORM pg_catalog.pg_advisory_xact_lock(71620261016);
    SELECT * INTO batch FROM public.batches WHERE id = p_batch_id FOR UPDATE;
    IF NOT FOUND OR batch.status <> 'available' THEN
        RAISE EXCEPTION 'Batch is no longer available';
    END IF;
    IF coalesce(cardinality(batch.pr_numbers), 0) = 0 THEN RAISE EXCEPTION 'Empty batch'; END IF;
    SELECT count(*) INTO found_count FROM public.prs WHERE number = ANY(batch.pr_numbers);
    IF found_count <> cardinality(batch.pr_numbers) THEN RAISE EXCEPTION 'Incomplete batch'; END IF;
    IF EXISTS (SELECT 1 FROM public.prs WHERE number = ANY(batch.pr_numbers)
        AND (assigned_user_id IS NOT NULL OR coalesce(assigned_reviewer, 'none') <> 'none'
             OR coalesce(is_draft, false))) THEN
        RAISE EXCEPTION 'Batch contains assigned or draft PRs';
    END IF;
    IF EXISTS (
        SELECT 1 FROM public.prs candidate
        JOIN public.prs active ON public.effective_code_files(to_jsonb(candidate.files)) && public.effective_code_files(to_jsonb(active.files))
        WHERE candidate.number = ANY(batch.pr_numbers) AND active.github_state NOT IN ('closed', 'merged')
        AND (active.assigned_user_id IS NOT NULL OR coalesce(active.assigned_reviewer, 'none') <> 'none')
    ) THEN RAISE EXCEPTION 'Files overlap an active assignment'; END IF;
    IF coalesce(batch.batch_type, 'disjoint') = 'disjoint' THEN
        IF EXISTS (
            SELECT 1 FROM public.prs a JOIN public.prs b
                ON a.number < b.number
                AND to_jsonb(a.files) ?| ARRAY(SELECT jsonb_array_elements_text(to_jsonb(b.files)))
            WHERE a.number = ANY(batch.pr_numbers) AND b.number = ANY(batch.pr_numbers)
        ) THEN RAISE EXCEPTION 'Batch contains overlapping files'; END IF;
    ELSIF batch.batch_type = 'soft_disjoint' THEN
        IF EXISTS (
            SELECT 1 FROM public.prs a JOIN public.prs b
                ON a.number < b.number
                AND public.effective_code_files(to_jsonb(a.files)) && public.effective_code_files(to_jsonb(b.files))
            WHERE a.number = ANY(batch.pr_numbers) AND b.number = ANY(batch.pr_numbers)
        ) THEN RAISE EXCEPTION 'Batch contains overlapping code files'; END IF;
    END IF;
    SELECT coalesce(identity_data->>'user_name', identity_data->>'preferred_username', auth.uid()::text)
        INTO username FROM auth.identities WHERE user_id = auth.uid() AND provider = 'github' LIMIT 1;
    IF username IS NULL THEN RAISE EXCEPTION 'GitHub identity required'; END IF;
    UPDATE public.batches SET status = 'claimed', claimed_by = username,
        claimed_user_id = auth.uid(), claimed_at = now(), updated_at = now() WHERE id = p_batch_id;
    UPDATE public.prs SET assigned_reviewer = username, assigned_user_id = auth.uid(),
        status = 'in-review', updated_at = now() WHERE number = ANY(batch.pr_numbers);
END $$;

CREATE OR REPLACE FUNCTION public.complete_batch(p_batch_id text)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE batch public.batches%ROWTYPE;
BEGIN
    IF auth.uid() IS NULL THEN RAISE EXCEPTION 'Sign in required'; END IF;
    PERFORM pg_catalog.pg_advisory_xact_lock(71620261016);
    SELECT * INTO batch FROM public.batches WHERE id = p_batch_id FOR UPDATE;
    IF NOT FOUND OR batch.claimed_user_id IS DISTINCT FROM auth.uid() THEN
        RAISE EXCEPTION 'Only the batch owner may complete it';
    END IF;
    -- Mark merged/closed PRs as completed while releasing any remaining open PRs back to the pool.
    UPDATE public.prs SET status = CASE WHEN github_state = 'merged' THEN 'merged'
                                        WHEN github_state = 'closed' THEN 'closed'
                                        ELSE 'unassigned' END,
        assigned_reviewer = CASE WHEN github_state IN ('merged', 'closed') THEN assigned_reviewer ELSE 'none' END,
        assigned_user_id = CASE WHEN github_state IN ('merged', 'closed') THEN assigned_user_id ELSE NULL END,
        updated_at = now()
        WHERE number = ANY(batch.pr_numbers) AND assigned_user_id = auth.uid();
    DELETE FROM public.batches WHERE id = p_batch_id;
END $$;

REVOKE ALL ON FUNCTION public.claim_batch(text), public.release_batch(text), public.complete_batch(text) FROM PUBLIC, anon;
GRANT EXECUTE ON FUNCTION public.claim_batch(text), public.release_batch(text), public.complete_batch(text) TO authenticated;

NOTIFY pgrst, 'reload schema';
COMMIT;
