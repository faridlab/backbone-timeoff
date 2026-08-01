-- Down: remove the company RLS fence for timeoff module

-- Reverse the company RLS fence for timeoff.timeoff_balances
DROP POLICY IF EXISTS timeoff_balances_company_isolation ON timeoff.timeoff_balances;
ALTER TABLE timeoff.timeoff_balances NO FORCE ROW LEVEL SECURITY;
ALTER TABLE timeoff.timeoff_balances DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for timeoff.timeoff_requests
DROP POLICY IF EXISTS timeoff_requests_company_isolation ON timeoff.timeoff_requests;
ALTER TABLE timeoff.timeoff_requests NO FORCE ROW LEVEL SECURITY;
ALTER TABLE timeoff.timeoff_requests DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for timeoff.timeoff_types
DROP POLICY IF EXISTS timeoff_types_company_isolation ON timeoff.timeoff_types;
ALTER TABLE timeoff.timeoff_types NO FORCE ROW LEVEL SECURITY;
ALTER TABLE timeoff.timeoff_types DISABLE ROW LEVEL SECURITY;

