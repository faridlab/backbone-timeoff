-- Migration: Add nullable max_days_per_request to timeoff.timeoff_types
--
-- The per-request ceiling for one leave type: a request asking for more days
-- than this is refused at submit (a half day is 0.5). NULL = no per-request
-- cap; the balance remains the hard floor either way. Additive and nullable —
-- existing types read as uncapped until a policy sets one.

ALTER TABLE timeoff.timeoff_types
    ADD COLUMN IF NOT EXISTS max_days_per_request NUMERIC(8, 2)
    CHECK (max_days_per_request IS NULL OR max_days_per_request > 0);
