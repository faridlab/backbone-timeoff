-- Down: drop timeoff.timeoff_types table
DROP TABLE IF EXISTS timeoff.timeoff_types CASCADE;
DROP FUNCTION IF EXISTS timeoff.timeoff_types_audit_timestamp() CASCADE;
