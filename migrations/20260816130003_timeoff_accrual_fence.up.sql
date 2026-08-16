-- Company fence posture for the accrual entities (ADR-0014: strict, Wave 1 P1).
-- The generator emits create-table migrations for NEW entities but does not
-- include RLS policies on them — hand-authored here with the P0 template
-- (see 20260816130000_company_fence_strict.up.sql). Accrual plans and their
-- levels are company-private configuration: never cross companies. company_id
-- is scoped per request via `set_config('app.company_id', <uuid>, true)`; an
-- unset var sees zero rows (fail-closed). Requires the app to connect as a
-- non-superuser role; migrations/seeders run as the owner and bypass.

-- Migration: company row-level-security fence for timeoff.timeoff_accrual_plans

ALTER TABLE timeoff.timeoff_accrual_plans ENABLE ROW LEVEL SECURITY;
ALTER TABLE timeoff.timeoff_accrual_plans FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS timeoff_accrual_plans_company_isolation ON timeoff.timeoff_accrual_plans;
CREATE POLICY timeoff_accrual_plans_company_isolation ON timeoff.timeoff_accrual_plans
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);

-- Migration: company row-level-security fence for timeoff.timeoff_accrual_levels

ALTER TABLE timeoff.timeoff_accrual_levels ENABLE ROW LEVEL SECURITY;
ALTER TABLE timeoff.timeoff_accrual_levels FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS timeoff_accrual_levels_company_isolation ON timeoff.timeoff_accrual_levels;
CREATE POLICY timeoff_accrual_levels_company_isolation ON timeoff.timeoff_accrual_levels
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
