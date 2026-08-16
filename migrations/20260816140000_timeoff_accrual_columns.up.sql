-- Accrual columns on timeoff_balances + the approvals seam column on timeoff_requests
-- (Wave 1 P1, H-2). The generator emits create-table migrations for NEW entities but does
-- not diff columns onto EXISTING tables — this is the hand-authored column migration, the
-- same posture as the P0 fence migrations.

-- Accrual walk state on the balance (the allocation): plan link, validity window,
-- watermark, postponed-carry, expiry stamp.
ALTER TABLE timeoff.timeoff_balances
    ADD COLUMN IF NOT EXISTS accrual_plan_id UUID REFERENCES timeoff.timeoff_accrual_plans(id),
    ADD COLUMN IF NOT EXISTS date_from DATE,
    ADD COLUMN IF NOT EXISTS date_to DATE,
    ADD COLUMN IF NOT EXISTS last_accrual_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS carried_over NUMERIC(18,2) NOT NULL DEFAULT 0 CHECK (carried_over >= 0),
    ADD COLUMN IF NOT EXISTS expired_at TIMESTAMPTZ;

-- The approvals seam (P1): links a request to its approvals.ApprovalRequest. Logical FK —
-- no DB constraint across module schemas (ADR-0004: modules compose via serialized ports).
ALTER TABLE timeoff.timeoff_requests
    ADD COLUMN IF NOT EXISTS approval_request_id UUID;

-- The accrual walk reads balances by plan + expiry state.
CREATE INDEX IF NOT EXISTS idx_timeoff_balances_accrual_plan
    ON timeoff.timeoff_balances (accrual_plan_id)
    WHERE accrual_plan_id IS NOT NULL;
