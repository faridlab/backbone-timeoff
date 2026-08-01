-- Down: drop timeoff.timeoff_requests table
DROP TABLE IF EXISTS timeoff.timeoff_requests CASCADE;
DROP FUNCTION IF EXISTS timeoff.timeoff_requests_audit_timestamp() CASCADE;
