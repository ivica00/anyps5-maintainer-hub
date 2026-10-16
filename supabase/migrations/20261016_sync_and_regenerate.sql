BEGIN;
ALTER TABLE public.prs ADD COLUMN IF NOT EXISTS github_state text NOT NULL DEFAULT 'unknown';
ALTER TABLE public.prs ADD COLUMN IF NOT EXISTS head_sha text;
-- Legacy rows stay unknown until a complete, explicit GitHub fetch confirms them.
ALTER TABLE public.prs ADD CONSTRAINT prs_github_state_valid
    CHECK (github_state IN ('unknown', 'open', 'closed', 'merged'));

CREATE OR REPLACE FUNCTION public.sync_github_snapshot(p_records jsonb, p_states jsonb)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE item jsonb;
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
    -- Missing values on upsert must never reset review notes or ownership.
    INSERT INTO public.prs (number, title, author, author_avatar, created_at, url, additions, deletions, total_lines, file_count, files, category, hotspots, risk_level, size, is_draft, head_sha, github_state)
    SELECT r.number, r.title, r.author, r.author_avatar, r.created_at, r.url, r.additions, r.deletions, r.total_lines, r.file_count, r.files, r.category, r.hotspots, r.risk_level, r.size, r.is_draft, r.head_sha, r.github_state
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
DECLARE current_prs jsonb; current_batches jsonb; item jsonb; numbers integer[];
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
        IF EXISTS (SELECT 1 FROM public.prs a JOIN public.prs b ON a.number < b.number
            AND to_jsonb(a.files) ?| ARRAY(SELECT jsonb_array_elements_text(to_jsonb(b.files)))
            WHERE a.number = ANY(numbers) AND b.number = ANY(numbers))
        THEN RAISE EXCEPTION 'Batch contains overlapping files'; END IF;
        INSERT INTO public.batches (id,title,category,complexity,complexity_label,pr_count,pr_numbers,status,claimed_by)
            VALUES (item->>'id', item->>'title', item->>'category', item->>'complexity',
                    item->>'complexity_label', cardinality(numbers), numbers, 'available', 'none');
    END LOOP;
END $$;
REVOKE ALL ON FUNCTION public.replace_available_batches(jsonb,jsonb,jsonb) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.replace_available_batches(jsonb,jsonb,jsonb) TO service_role;

-- Unknown/closed PRs and incomplete file metadata must never be claimed.
CREATE OR REPLACE FUNCTION public.check_claimable_batch()
RETURNS trigger LANGUAGE plpgsql SET search_path = '' AS $$
BEGIN
    IF NEW.status = 'claimed' AND (OLD.status IS DISTINCT FROM NEW.status
        OR OLD.pr_numbers IS DISTINCT FROM NEW.pr_numbers) THEN
        IF EXISTS (SELECT 1 FROM public.prs WHERE number = ANY(NEW.pr_numbers)
            AND (github_state <> 'open' OR coalesce(is_draft, false)
                 OR coalesce(jsonb_array_length(to_jsonb(files)), 0) = 0)) THEN
            RAISE EXCEPTION 'Batch contains closed, draft or unverified PRs';
        END IF;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER check_claimable_batch BEFORE UPDATE ON public.batches
    FOR EACH ROW EXECUTE FUNCTION public.check_claimable_batch();
NOTIFY pgrst, 'reload schema';
COMMIT;
