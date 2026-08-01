-- Cross-field drawdown invariant backstop for timeoff.timeoff_balances.
--
-- The `@non_negative` schema attribute already emits column CHECKs for
-- `allocated >= 0` and `used >= 0` in 20260426220001_create_timeoff_balance_table.up.sql.
-- The cross-field `used <= allocated` guard is NOT expressible in schema YAML
-- (the codegen has no `constraints:` block support), so it is added here as a
-- hand-authored migration — the exact pattern proven in backbone-hr's
-- 20260426220010_leave_balance_guards.up.sql. This makes the drawdown invariant
-- DB-enforced against ANY writer (the generic 12-endpoint CRUD stack, a raw
-- INSERT), not just the gated approve service path. Hand-authored (no generator
-- marker) — preserved across regen.
ALTER TABLE timeoff.timeoff_balances
  ADD CONSTRAINT timeoff_balances_used_within_allocated CHECK (used <= allocated);
