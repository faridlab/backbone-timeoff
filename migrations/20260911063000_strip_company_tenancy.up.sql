-- Hand-authored (user-owned). Not regenerated.
--
-- Strip every company-fence artifact from the timeoff tables (ADR-0029): the module is
-- tenant-agnostic; org scoping is installed by the COMPOSING service's tenancy decorator,
-- never by the module. Dropped here, per table: the company-leading indexes, the
-- <table>_company_isolation RLS policy, and the company_id column itself.
--
-- Tables: timeoff_types, timeoff_requests, timeoff_balances, timeoff_accrual_plans,
-- timeoff_accrual_levels.
--
-- Ordering guard (the decorator must run FIRST on any database with data): the module
-- never moves tenancy data. A table is safe to strip when EITHER
--   a) it carries org_unit_id with no NULLs — the decorator backfilled it from company_id —
--      or b) it is empty (a fresh database: the earlier chain files created it empty).
-- Otherwise the strip RAISEs, naming the decorator step, rather than dropping a column
-- that still holds the only tenancy key. The file is re-runnable (every drop is IF EXISTS
-- and the tracker has no checksums), so a failed run retries cleanly after the decorator
-- lands.
--
-- RLS enable/force flags are deliberately NOT touched: the decorator owns those now.
-- The tenant-free domain artifacts stay: the accrual-level uniqueness on
-- (plan_id, sequence), the request status index, and every metadata index key no tenant
-- column. The per-unit uniqueness postures are owned by the composing service's tenancy
-- decorator and are intentionally NOT restored by the down migration: the per-unit
-- (employee_id, timeoff_type_id, period) unique on balances is what lets the draw/restore
-- UPDATEs move exactly one row, and the per-unit (code) unique on types keeps one leave
-- vocabulary per unit — the pre-fence global forms would forbid two units of one tenant
-- from keeping their own.

DO $$
DECLARE
    t text;
    has_org boolean;
    org_nulls bigint;
    total bigint;
    offenders text := '';
BEGIN
    FOREACH t IN ARRAY ARRAY['timeoff_types', 'timeoff_requests', 'timeoff_balances', 'timeoff_accrual_plans', 'timeoff_accrual_levels']
    LOOP
        IF to_regclass(format('timeoff.%I', t)) IS NULL THEN
            CONTINUE; -- chain not fully applied on this database; nothing to strip
        END IF;

        SELECT EXISTS (
                   SELECT 1 FROM information_schema.columns
                   WHERE table_schema = 'timeoff' AND table_name = t AND column_name = 'org_unit_id'
               )
        INTO has_org;

        EXECUTE format('SELECT count(*) FROM timeoff.%I', t) INTO total;

        IF has_org THEN
            EXECUTE format(
                'SELECT count(*) FROM timeoff.%I WHERE org_unit_id IS NULL', t)
            INTO org_nulls;
        ELSE
            org_nulls := total; -- no org column: every row's only tenancy key is company_id
        END IF;

        IF has_org AND org_nulls = 0 THEN
            CONTINUE; -- decorator backfilled: safe
        END IF;
        IF total = 0 THEN
            CONTINUE; -- empty table (fresh database): safe
        END IF;
        offenders := offenders || format(' timeoff.%s (%s rows, %s rows not covered by org_unit_id);', t, total, org_nulls);
    END LOOP;

    IF offenders <> '' THEN
        RAISE EXCEPTION 'refusing to strip company_id — these tables are not yet covered by the tenancy decorator:%. Apply the composing service''s tenancy decorator (it backfills org_unit_id from company_id) and re-run; it is the only step that moves tenancy data.', offenders;
    END IF;
END $$;

-- ── timeoff_types ─────────────────────────────────────────────────────────────
DROP INDEX IF EXISTS timeoff.idx_timeoff_types_company_id_code;
DROP POLICY IF EXISTS timeoff_types_company_isolation ON timeoff.timeoff_types;
ALTER TABLE timeoff.timeoff_types DROP COLUMN IF EXISTS company_id;

-- ── timeoff_requests ──────────────────────────────────────────────────────────
DROP INDEX IF EXISTS timeoff.idx_timeoff_requests_company_id_employee_id_date_start;
DROP POLICY IF EXISTS timeoff_requests_company_isolation ON timeoff.timeoff_requests;
ALTER TABLE timeoff.timeoff_requests DROP COLUMN IF EXISTS company_id;

-- ── timeoff_balances ──────────────────────────────────────────────────────────
DROP INDEX IF EXISTS timeoff.idx_timeoff_balances_company_id_employee_id_timeoff_type_id_period;
DROP POLICY IF EXISTS timeoff_balances_company_isolation ON timeoff.timeoff_balances;
ALTER TABLE timeoff.timeoff_balances DROP COLUMN IF EXISTS company_id;

-- ── timeoff_accrual_plans ─────────────────────────────────────────────────────
DROP INDEX IF EXISTS timeoff.idx_timeoff_accrual_plans_company_id_timeoff_type_id;
DROP POLICY IF EXISTS timeoff_accrual_plans_company_isolation ON timeoff.timeoff_accrual_plans;
ALTER TABLE timeoff.timeoff_accrual_plans DROP COLUMN IF EXISTS company_id;

-- ── timeoff_accrual_levels ────────────────────────────────────────────────────
DROP POLICY IF EXISTS timeoff_accrual_levels_company_isolation ON timeoff.timeoff_accrual_levels;
ALTER TABLE timeoff.timeoff_accrual_levels DROP COLUMN IF EXISTS company_id;
