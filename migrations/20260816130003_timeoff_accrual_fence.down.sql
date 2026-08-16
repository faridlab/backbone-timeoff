-- Revert the ADR-0014 strict company fence for the accrual entities (Wave 1 P1).
DROP POLICY IF EXISTS timeoff_accrual_plans_company_isolation ON timeoff.timeoff_accrual_plans;
ALTER TABLE timeoff.timeoff_accrual_plans NO FORCE ROW LEVEL SECURITY;
ALTER TABLE timeoff.timeoff_accrual_plans DISABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS timeoff_accrual_levels_company_isolation ON timeoff.timeoff_accrual_levels;
ALTER TABLE timeoff.timeoff_accrual_levels NO FORCE ROW LEVEL SECURITY;
ALTER TABLE timeoff.timeoff_accrual_levels DISABLE ROW LEVEL SECURITY;
