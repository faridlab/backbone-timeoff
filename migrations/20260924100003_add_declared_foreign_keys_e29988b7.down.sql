-- Down: drop the declared foreign keys for timeoff module
ALTER TABLE IF EXISTS timeoff.timeoff_requests DROP CONSTRAINT IF EXISTS fk_timeoff_requests_timeoff_type_id;
ALTER TABLE IF EXISTS timeoff.timeoff_balances DROP CONSTRAINT IF EXISTS fk_timeoff_balances_accrual_plan_id;
ALTER TABLE IF EXISTS timeoff.timeoff_balances DROP CONSTRAINT IF EXISTS fk_timeoff_balances_timeoff_type_id;
ALTER TABLE IF EXISTS timeoff.timeoff_accrual_levels DROP CONSTRAINT IF EXISTS fk_timeoff_accrual_levels_plan_id;
ALTER TABLE IF EXISTS timeoff.timeoff_accrual_plans DROP CONSTRAINT IF EXISTS fk_timeoff_accrual_plans_timeoff_type_id;
