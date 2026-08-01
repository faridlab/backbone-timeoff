-- Reverse the timeoff_balances cross-field drawdown guard.
ALTER TABLE timeoff.timeoff_balances
  DROP CONSTRAINT IF EXISTS timeoff_balances_used_within_allocated;
