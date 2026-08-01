-- Down: drop timeoff.timeoff_balances table
DROP TABLE IF EXISTS timeoff.timeoff_balances CASCADE;
DROP FUNCTION IF EXISTS timeoff.timeoff_balances_audit_timestamp() CASCADE;
