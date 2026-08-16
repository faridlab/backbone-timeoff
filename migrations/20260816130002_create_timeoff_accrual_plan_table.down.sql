-- Down: drop timeoff.timeoff_accrual_plans table
DROP TABLE IF EXISTS timeoff.timeoff_accrual_plans CASCADE;
DROP FUNCTION IF EXISTS timeoff.timeoff_accrual_plans_audit_timestamp() CASCADE;
