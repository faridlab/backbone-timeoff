-- Revert the accrual columns on timeoff_balances + the approvals seam column on
-- timeoff_requests (Wave 1 P1, H-2). Drops the walk state, the plan link, the
-- validity window, the watermark, the postponed carry, the expiry stamp, and the
-- approvals.ApprovalRequest link. Data loss on downgrade — deliberate.

DROP INDEX IF EXISTS timeoff.idx_timeoff_balances_accrual_plan;

ALTER TABLE timeoff.timeoff_requests
    DROP COLUMN IF EXISTS approval_request_id;

ALTER TABLE timeoff.timeoff_balances
    DROP COLUMN IF EXISTS expired_at,
    DROP COLUMN IF EXISTS carried_over,
    DROP COLUMN IF EXISTS last_accrual_at,
    DROP COLUMN IF EXISTS date_to,
    DROP COLUMN IF EXISTS date_from,
    DROP COLUMN IF EXISTS accrual_plan_id;
