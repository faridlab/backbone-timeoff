# Accrual engine + approvals seam (Wave 1 P1, H-2)

Ported from Odoo `hr_holidays`' accrued-allocation machinery
(`hr_leave_allocation_cron_accrual`, `hr.leave.accrual.plan` /
`.level`) per the extraction spec (HLB-16/17/18, HLM-10), plus the P1
approvals seam toward backbone-approvals (H-9 lands in P6). Source of
truth for the *what*: `docs/odoo/human-resources/hr/hr-holidays-expense-business-logic.md` §1.

## The model (schema)

- `timeoff_accrual_plans` — a named plan bound to a timeoff type.
- `timeoff_accrual_levels` — the plan's **tenure ladder**, ordered by
  `sequence`; unique `(plan_id, sequence)` among live rows. Each rung:
  - `start_count × start_type` (days/months/years) — how much tenure (from
    the allocation start) must have elapsed for the rung to apply;
  - `frequency` (daily/weekly/biweekly/monthly/bimonthly/quarterly/yearly/once)
    and `added_value` — what a period grants;
  - `maximum_leave` — cap on the balance's `allocated`;
  - `action_with_lost_days` (`nothing` | `postponed_to_next_accrual`) +
    `postponed_max_days` — what happens to over-cap days;
  - `is_added_based_on_worked_time` — DEFERRED: grants full `added_value`
    until the attendance integration lands (Odoo does the same without
    `hr_attendance` installed).
- `timeoff_balances` gains the walk state: `accrual_plan_id`, validity
  window `date_from`/`date_to`, watermark `last_accrual_at` (spec's
  `lastleaves_updated`), `carried_over` (postponed days, ≥ 0), `expired_at`.
- `timeoff_requests` gains `approval_request_id` — the approvals seam link
  (logical FK, no DB constraint across module schemas; ADR-0004).

## The walk (`AccrualService::run_accrual`, `scheduled_jobs.accrual_update`)

Per balance, in one short tx per row (`commit_policy: commit_per_batch`):

1. **Expiry first**: past `date_to` → stamp `expired_at`, stop accruing.
2. **Applicable rung** = the LATEST level whose `start_count × start_type`
   from `date_from` (the allocation start — the spec's reference point, NOT
   the employment join date) has elapsed. A ladder with no eligible rung
   (forgot a start-0 rung) skips loudly — grants nothing.
3. **Periods due** = whole frequency periods since the watermark
   (`last_accrual_at`, or midnight of `date_from` on the first run).
   Month-family periods are calendar months on the anniversary day,
   clamped (Jan 31 + 1 mo → Feb 28/29). Zero periods → not due, no write.
4. **Grant** = `added_value × periods` + the `carried_over` released from
   the previous pass (HLB-18: postponed days re-grant at the NEXT accrual).
5. **Cap**: grant min(gross, `maximum_leave − allocated`); over-cap excess
   follows `action_with_lost_days`: held in `carried_over` bounded by
   `postponed_max_days` (rest lost), or dropped.
6. **Watermark** advances by exactly the granted periods — never to `now` —
   so remainders are never silently absorbed.

`once` grants one time only (watermark set ⇒ never again).

**Concurrency** (`pickup_lock: true`, ADR-0020): the claim is
`FOR UPDATE SKIP LOCKED` in a short claim tx; every apply is additionally
guarded by `last_accrual_at IS NOT DISTINCT FROM <claimed value>`, so two
concurrent walks can never double-grant a row (the loser's update matches
zero rows and is counted as raced/skipped). The walk is a background job
crossing all tenants: each row's apply tx relays the ambient org scope the
composing service bound (`org_scope::bind_org_scope_on`) when one is bound,
so the decorator-installed fence holds end-to-end; an undecorated
deployment applies plain.

**Posture** (`posture: self_arming`): the daily 03:00 schedule is a FLOOR.
The composing app arms the job from `timeoff_balance_created` /
`timeoff_balance_updated` (a balance gaining a plan or its window moving
should arm within minutes, not a day).

## The approvals seam (`approvals_port.rs`)

ADR-0004 forbids a crate edge on backbone-approvals, so the link is data +
a port:

- `ApprovalFiling` (async trait): `file(filing) -> approval_request_id`,
  `status(id) -> ApprovalVerdict`. The composing app implements it against
  the H-9 engine when that lands; `UnwiredApprovals` is the default.
- `TimeoffRequestWriteService::submit_request(...)` — creates the request
  `pending`; when a port is wired, files it first (the filing carries the
  client-generated request id) and inserts the row already linked. A wired
  port that fails transport fails the submit — no silently untracked
  request. The unwired default keeps pre-P1 behavior: no link, direct
  manager approval.
- `approve_request` (TR2): a request carrying `approval_request_id` is
  granted ONLY when the linked filing's verdict is `approved`. Pending →
  `ApprovalNotGranted`; an unwired port with a linked request (out-of-band
  linkage) fails CLOSED. TR1 (balance draw gate) and TR3 (cancel restores)
  are unchanged from the P0 port.

## Draw gate

`TimeoffBalanceRepository::draw` now also requires the request's
`[date_start, date_end]` to fall inside the balance's `[date_from, date_to]`
(open bounds when NULL — manual allocations have no window). Restore is
deliberately ungated: the days were drawn from that balance, so returning
them can never manufacture entitlement.

## Generator limitations this port works around

(established in P0; still true)

- `schema generate` (even `--force`) emits create-table migrations for NEW
  entities but no RLS on them and no column diffs onto EXISTING tables.
  Hand-authored: `20260816130003_timeoff_accrual_fence` (fence for the two
  accrual tables, byte-exact P0 template) and
  `20260816140000_timeoff_accrual_columns` (the balance/request columns).
- `--force` regen re-emits the `example_saga_workflow` scaffold into
  `src/application/workflows/mod.rs`; restore that file from git after
  regen (safe-regen procedure).

## Proven by

`tests/accrual_test.rs` (11 cases, live-pool against a migrated
`backbone_timeoff_test`): monthly grant math + watermark advance;
idempotency + delta-only grants; cap + bounded postpone + re-grant + loss;
`nothing` drops excess; validity expiry; tenure ladder picks the latest
eligible rung; `once` single grant; wired seam files + TR2 blocks until
the verdict flips; unwired seam keeps pre-P1 behavior; draw gate honors
the validity window; fence policies present on all five fenced tables.
