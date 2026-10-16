BEGIN;

-- Membership is managed only by the project administrator, using Auth user UUIDs.
CREATE TABLE IF NOT EXISTS public.hub_maintainers (
    user_id uuid PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE
);
ALTER TABLE public.hub_maintainers ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON public.hub_maintainers FROM PUBLIC, anon, authenticated;

ALTER TABLE public.batches ADD COLUMN IF NOT EXISTS claimed_user_id uuid REFERENCES auth.users(id);
ALTER TABLE public.prs ADD COLUMN IF NOT EXISTS assigned_user_id uuid REFERENCES auth.users(id);

-- Replace permissive legacy policies, rather than adding restrictive policies beside them.
DO $$
DECLARE item record;
BEGIN
    FOR item IN SELECT tablename, policyname FROM pg_policies
        WHERE schemaname = 'public' AND tablename IN ('prs', 'batches')
    LOOP
        EXECUTE format('DROP POLICY %I ON public.%I', item.policyname, item.tablename);
    END LOOP;
END $$;
ALTER TABLE public.prs ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.batches ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON public.prs, public.batches FROM PUBLIC, anon, authenticated;
GRANT SELECT ON public.prs, public.batches TO anon, authenticated;
GRANT UPDATE (status, notes, checklist, updated_at) ON public.prs TO authenticated;
CREATE POLICY read_prs ON public.prs FOR SELECT TO anon, authenticated USING (true);
CREATE POLICY read_batches ON public.batches FOR SELECT TO anon, authenticated USING (true);
CREATE POLICY edit_owned_pr ON public.prs FOR UPDATE TO authenticated
    USING (assigned_user_id = (SELECT auth.uid()))
    WITH CHECK (assigned_user_id = (SELECT auth.uid()));

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
        WHERE candidate.number = ANY(batch.pr_numbers) AND (active.assigned_user_id IS NOT NULL OR coalesce(active.assigned_reviewer, 'none') <> 'none')
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

CREATE OR REPLACE FUNCTION public.release_batch(p_batch_id text)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE batch public.batches%ROWTYPE;
BEGIN
    IF auth.uid() IS NULL THEN RAISE EXCEPTION 'Sign in required'; END IF;
    PERFORM pg_catalog.pg_advisory_xact_lock(71620261016);
    SELECT * INTO batch FROM public.batches WHERE id = p_batch_id FOR UPDATE;
    IF NOT FOUND OR batch.claimed_user_id IS DISTINCT FROM auth.uid() THEN
        RAISE EXCEPTION 'Only the batch owner may release it';
    END IF;
    UPDATE public.prs SET assigned_reviewer = 'none', assigned_user_id = NULL,
        status = 'unassigned', updated_at = now()
        WHERE number = ANY(batch.pr_numbers) AND assigned_user_id = auth.uid();
    UPDATE public.batches SET status = 'available', claimed_by = 'none', claimed_user_id = NULL,
        claimed_at = NULL, updated_at = now() WHERE id = p_batch_id;
END $$;
REVOKE ALL ON FUNCTION public.claim_batch(text), public.release_batch(text) FROM PUBLIC, anon;
GRANT EXECUTE ON FUNCTION public.claim_batch(text), public.release_batch(text) TO authenticated;
NOTIFY pgrst, 'reload schema';
COMMIT;
