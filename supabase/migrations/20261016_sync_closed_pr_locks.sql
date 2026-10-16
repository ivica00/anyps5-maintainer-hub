BEGIN;
-- Completed PRs remain in history but no longer reserve files for new claims.
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
        JOIN public.prs active ON to_jsonb(candidate.files) ?| ARRAY(SELECT jsonb_array_elements_text(to_jsonb(active.files)))
        WHERE candidate.number = ANY(batch.pr_numbers) AND active.github_state NOT IN ('closed', 'merged')
        AND (active.assigned_user_id IS NOT NULL OR coalesce(active.assigned_reviewer, 'none') <> 'none')
    ) THEN RAISE EXCEPTION 'Files overlap an active assignment'; END IF;
    IF EXISTS (
        SELECT 1 FROM public.prs a JOIN public.prs b
            ON a.number < b.number
            AND to_jsonb(a.files) ?| ARRAY(SELECT jsonb_array_elements_text(to_jsonb(b.files)))
        WHERE a.number = ANY(batch.pr_numbers) AND b.number = ANY(batch.pr_numbers)
    ) THEN RAISE EXCEPTION 'Batch contains overlapping files'; END IF;
    SELECT coalesce(identity_data->>'user_name', identity_data->>'preferred_username', auth.uid()::text)
        INTO username FROM auth.identities WHERE user_id = auth.uid() AND provider = 'github' LIMIT 1;
    IF username IS NULL THEN RAISE EXCEPTION 'GitHub identity required'; END IF;
    UPDATE public.batches SET status = 'claimed', claimed_by = username,
        claimed_user_id = auth.uid(), claimed_at = now(), updated_at = now() WHERE id = p_batch_id;
    UPDATE public.prs SET assigned_reviewer = username, assigned_user_id = auth.uid(),
        status = 'in-review', updated_at = now() WHERE number = ANY(batch.pr_numbers);
END $$;

NOTIFY pgrst, 'reload schema';
COMMIT;
