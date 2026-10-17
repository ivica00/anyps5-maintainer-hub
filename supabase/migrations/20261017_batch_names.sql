BEGIN;
SELECT pg_advisory_xact_lock(71620261016);
LOCK TABLE public.batches IN SHARE ROW EXCLUSIVE MODE;

-- Keep names stable even when available batches are deleted and regenerated.
CREATE TABLE public.batch_names (
    batch_id text PRIMARY KEY,
    batch_number bigint GENERATED ALWAYS AS IDENTITY UNIQUE NOT NULL
);
ALTER TABLE public.batch_names ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON public.batch_names FROM PUBLIC, anon, authenticated;
GRANT SELECT ON public.batch_names TO service_role;

INSERT INTO public.batch_names(batch_id) SELECT id FROM public.batches ORDER BY id;

CREATE OR REPLACE FUNCTION public.batch_title_for_id(p_id text)
RETURNS text LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
DECLARE n bigint;
BEGIN
    SELECT batch_number INTO n FROM public.batch_names WHERE batch_id = p_id;
    IF n IS NULL THEN
        INSERT INTO public.batch_names(batch_id) VALUES (p_id)
        ON CONFLICT (batch_id) DO UPDATE SET batch_id = EXCLUDED.batch_id
        RETURNING batch_number INTO n;
    END IF;
    RETURN 'Batch ' || n::text;
END $$;
REVOKE ALL ON FUNCTION public.batch_title_for_id(text) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.batch_title_for_id(text) TO service_role;

CREATE OR REPLACE FUNCTION public.set_batch_title()
RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path = '' AS $$
BEGIN
    NEW.title := public.batch_title_for_id(NEW.id);
    RETURN NEW;
END $$;
REVOKE ALL ON FUNCTION public.set_batch_title() FROM PUBLIC, anon, authenticated;
CREATE TRIGGER set_batch_title BEFORE INSERT OR UPDATE OF id, title ON public.batches
    FOR EACH ROW EXECUTE FUNCTION public.set_batch_title();
UPDATE public.batches SET title = public.batch_title_for_id(id);

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
            VALUES (item->>'id', public.batch_title_for_id(item->>'id'), item->>'category', item->>'complexity',
                    item->>'complexity_label', cardinality(numbers), numbers, 'available', 'none');
    END LOOP;
END $$;
REVOKE ALL ON FUNCTION public.replace_available_batches(jsonb,jsonb,jsonb) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.replace_available_batches(jsonb,jsonb,jsonb) TO service_role;

NOTIFY pgrst, 'reload schema';
COMMIT;
