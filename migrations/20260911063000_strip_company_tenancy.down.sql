-- Hand-authored (user-owned). Not regenerated.
--
-- Best-effort restore sketch for the tenancy strip (ADR-0029). This is a breaking module
-- release against dev-stage databases: the down re-adds the company_id column as nullable
-- with the company isolation policy shape, but restores NO data — rows written after the
-- strip (or after the decorator re-keyed them) carry org_unit_id only. The composing
-- service's tenancy decorator remains the live fence; treat this down as a schema-shape
-- sketch for archaeology, not a usable rollback. The per-unit uniqueness postures (balances
-- on employee/type/period, types on code) are NOT restored — see the up migration.

ALTER TABLE timeoff.timeoff_types          ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE timeoff.timeoff_requests       ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE timeoff.timeoff_balances       ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE timeoff.timeoff_accrual_plans  ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE timeoff.timeoff_accrual_levels ADD COLUMN IF NOT EXISTS company_id uuid;

CREATE UNIQUE INDEX IF NOT EXISTS idx_timeoff_types_company_id_code
    ON timeoff.timeoff_types (company_id, code) WHERE (metadata->>'deleted_at') IS NULL;
CREATE INDEX IF NOT EXISTS idx_timeoff_requests_company_id_employee_id_date_start
    ON timeoff.timeoff_requests (company_id, employee_id, date_start);
CREATE UNIQUE INDEX IF NOT EXISTS idx_timeoff_balances_company_id_employee_id_timeoff_type_id_period
    ON timeoff.timeoff_balances (company_id, employee_id, timeoff_type_id, period) WHERE (metadata->>'deleted_at') IS NULL;
CREATE INDEX IF NOT EXISTS idx_timeoff_accrual_plans_company_id_timeoff_type_id
    ON timeoff.timeoff_accrual_plans (company_id, timeoff_type_id);

CREATE POLICY timeoff_types_company_isolation ON timeoff.timeoff_types
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY timeoff_requests_company_isolation ON timeoff.timeoff_requests
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY timeoff_balances_company_isolation ON timeoff.timeoff_balances
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY timeoff_accrual_plans_company_isolation ON timeoff.timeoff_accrual_plans
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY timeoff_accrual_levels_company_isolation ON timeoff.timeoff_accrual_levels
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
