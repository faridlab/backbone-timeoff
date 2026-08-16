-- Down: drop timeoff.timeoff_accrual_levels table
DROP TABLE IF EXISTS timeoff.timeoff_accrual_levels CASCADE;
DROP FUNCTION IF EXISTS timeoff.timeoff_accrual_levels_audit_timestamp() CASCADE;
