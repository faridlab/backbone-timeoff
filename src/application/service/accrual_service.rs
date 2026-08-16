//! The accrual engine (Wave 1 P1, H-2) — user-owned (declared under
//! `user_owned` in `metaphor.codegen.yaml`).
//!
//! Ported from Odoo `hr.leave.allocation`'s daily
//! `hr_leave_allocation_cron_accrual` walk (HLB-16/17/18, HLM-10):
//!
//! 1. walk active plan-linked balances (claim batch, `FOR UPDATE SKIP LOCKED`);
//! 2. find the applicable level — the LATEST ladder rung whose
//!    `start_count × start_type` from the ALLOCATION start (`date_from`) has
//!    elapsed (a plan escalates 1.5 d/mo → 2 d/mo on tenure automatically);
//! 3. grant `added_value` per whole elapsed frequency period since the
//!    `last_accrual_at` watermark (first run: since `date_from` — Odoo accrues
//!    from the allocation date);
//! 4. cap at `maximum_leave`; over-cap days follow `action_with_lost_days`:
//!    `nothing` (discarded) or `postponed_to_next_accrual` (held in
//!    `carried_over`, bounded by `postponed_max_days`, re-granted next walk);
//! 5. past `date_to` → stamp `expired_at` and stop accruing;
//! 6. advance the watermark by the granted periods (NOT to `now` — remainders
//!    are never silently absorbed).
//!
//! Scheduled as `accrual_update` (`posture: self_arming`, ADR-0020): the
//! daily 03:00 schedule is a FLOOR; the composing app arms it from
//! `timeoff_balance_created/updated` too. `commit_policy: commit_per_batch`:
//! every balance's grant is its own short transaction, so one poisoned row
//! never rolls back the walk.
//!
//! Deferral: `is_added_based_on_worked_time` (proportional grant from
//! attendance) needs the attendance module's port and lands with that
//! integration — until then such levels grant the full `added_value` (Odoo
//! grants full when `hr_attendance` is not installed; same behavior).

use backbone_orm::company_scope;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use sqlx::PgPool;
use tracing::warn;

use crate::infrastructure::persistence::{
    AccrualLevelRow, AccrualWalkRow, TimeoffAccrualRepository,
};

#[derive(Debug, thiserror::Error)]
pub enum AccrualError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
}

/// One walk run's tally. `skipped` rows are counted with their reason so an
/// operator can see a misconfigured plan ladder (no eligible rung) instead of
/// silence that reads as "nothing due".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AccrualRunOutcome {
    pub claimed: usize,
    pub granted_rows: usize,
    pub granted_days: Decimal,
    pub postponed_days: Decimal,
    pub lost_days: Decimal,
    pub expired_rows: usize,
    pub not_due_rows: usize,
    pub skipped_rows: usize,
}

/// The accrual walk. Construct per composing app (or per test); `run_accrual`
/// is the `scheduled_jobs.accrual_update` handler.
pub struct AccrualService {
    pool: PgPool,
    walk: TimeoffAccrualRepository,
}

impl AccrualService {
    pub fn new(pool: PgPool) -> Self {
        let walk = TimeoffAccrualRepository::new(pool.clone());
        Self { pool, walk }
    }

    /// Run the walk until dry or `max_batches` claim passes. `now` is injected
    /// (tests pin it; the scheduler passes `Utc::now()`).
    pub async fn run_accrual(
        &self,
        now: DateTime<Utc>,
        batch: i64,
        max_batches: usize,
    ) -> Result<AccrualRunOutcome, AccrualError> {
        let mut total = AccrualRunOutcome::default();
        for _ in 0..max_batches {
            // Claim pass: short tx, SKIP LOCKED, read state, commit — then each
            // grant applies in its OWN tx (commit per batch row, ADR-0020).
            let rows = {
                let mut tx = self.pool.begin().await?;
                let rows = self.walk.claim_batch_for_walk(&mut tx, batch).await?;
                tx.commit().await?;
                rows
            };
            if rows.is_empty() {
                break;
            }
            total.claimed += rows.len();
            for row in &rows {
                let mut tx = self.pool.begin().await?;
                // The walk is a background job crossing all companies: bind the
                // row's own company so the apply rides the ADR-0014 fence.
                company_scope::bind_company_on(&mut tx, row.company_id).await?;
                let one = match self.process_row(&mut tx, row, now).await {
                    Ok(o) => o,
                    Err(e) => {
                        // One poisoned row rolls back only itself; the walk
                        // continues (commit_per_batch, ADR-0020).
                        tx.rollback().await?;
                        total.skipped_rows += 1;
                        tracing::error!(balance_id = %row.id, error = %e, "accrual walk: row failed");
                        continue;
                    }
                };
                // Every verdict is a committed state (granted / expired /
                // not-due / skipped-raced are all terminal for this pass).
                tx.commit().await?;
                total.absorb(one);
            }
        }
        Ok(total)
    }

    /// One balance's verdict for this pass. Committing or rolling back the
    /// caller's tx is the CALLER's job (commit-per-row); this returns the
    /// outcome either way.
    async fn process_row(
        &self,
        tx: &mut sqlx::PgConnection,
        row: &AccrualWalkRow,
        now: DateTime<Utc>,
    ) -> Result<RowOutcome, AccrualError> {
        // Step 5 first: past the validity window, the balance expires — no
        // partial-period grant, matching Odoo's deactivate-on-expiry.
        if let Some(date_to) = row.date_to {
            if date_to < now.date_naive() {
                self.walk.stamp_expired(tx, row.id, now).await?;
                return Ok(RowOutcome::Expired);
            }
        }

        // The ladder's reference point is the allocation start. A plan-linked
        // balance without one is misconfigured — skip loudly, grant nothing.
        let date_from = match row.date_from {
            Some(d) => d,
            None => {
                warn!(
                    balance_id = %row.id,
                    "accrual walk: plan-linked balance without date_from — skipped"
                );
                return Ok(RowOutcome::Skipped);
            }
        };

        let levels = self.walk.find_levels_for_plan(tx, row.accrual_plan_id).await?;
        // The applicable rung: the LATEST whose start offset from date_from
        // has elapsed. No rung elapsed (a ladder that forgets a start-0 rung)
        // is a configuration gap — skip, never grant.
        let level = match applicable_level(&levels, date_from, now) {
            Some(l) => l,
            None => {
                warn!(
                    balance_id = %row.id,
                    "accrual walk: no applicable accrual level yet — skipped"
                );
                return Ok(RowOutcome::Skipped);
            }
        };

        // Watermark: last accrual, or the allocation start on the first walk.
        let watermark = row.last_accrual_at.unwrap_or_else(|| date_from.and_hms_opt(0, 0, 0).unwrap().and_utc());

        // `once` grants exactly one time — a watermark already set means that
        // time has passed (guards re-grant even if the rung's period math would
        // keep counting elapsed "months").
        if level.frequency == "once" {
            if row.last_accrual_at.is_some() {
                return Ok(RowOutcome::NotDue);
            }
            let granted = level.added_value.min(headroom(level, row.allocated));
            let excess = level.added_value - granted;
            let (carried, lost) = split_excess(level, excess);
            let moved = self
                .walk
                .apply_grant(tx, row.id, row.last_accrual_at, granted, carried, now)
                .await?;
            return Ok(if moved == 1 {
                RowOutcome::Granted { granted, carried, lost }
            } else {
                RowOutcome::Raced
            });
        }

        // Whole elapsed periods since the watermark (HLM-10 frequencies).
        let periods = match periods_due(level.frequency.as_str(), watermark, now) {
            Some(p) if p > 0 => p,
            Some(_) => return Ok(RowOutcome::NotDue),
            None => {
                warn!(
                    balance_id = %row.id,
                    frequency = %level.frequency,
                    "accrual walk: unknown frequency — skipped"
                );
                return Ok(RowOutcome::Skipped);
            }
        };

        // Gross = this pass's grants + the postponed days held from the
        // previous pass (HLB-18: postponed days are re-granted at the NEXT
        // accrual, subject to the same cap).
        let grant_gross = level.added_value * Decimal::from(periods);
        let released_carry = row.carried_over;
        let gross = grant_gross + released_carry;

        // Cap (step 3/4): over-cap days follow action_with_lost_days.
        let room = headroom(level, row.allocated);
        let granted = gross.min(room);
        let excess = gross - granted;
        let (carried, lost) = split_excess(level, excess);

        // Advance the watermark by exactly the granted periods (step 6).
        let new_watermark = advance_watermark(level.frequency.as_str(), watermark, periods)
            .unwrap_or(now);

        let moved = self
            .walk
            .apply_grant(tx, row.id, row.last_accrual_at, granted, carried, new_watermark)
            .await?;
        Ok(if moved == 1 {
            RowOutcome::Granted { granted, carried, lost }
        } else {
            // Another walker advanced this row between our claim-commit and
            // this apply — its grant is as good as ours.
            RowOutcome::Raced
        })
    }
}

/// One row's verdict for the run tally.
#[derive(Debug, Clone, PartialEq)]
enum RowOutcome {
    Granted { granted: Decimal, carried: Decimal, lost: Decimal },
    Expired,
    NotDue,
    Skipped,
    Raced,
}

impl AccrualRunOutcome {
    fn absorb(&mut self, one: RowOutcome) {
        match one {
            RowOutcome::Granted { granted, carried, lost } => {
                self.granted_rows += 1;
                self.granted_days += granted;
                self.postponed_days += carried;
                self.lost_days += lost;
            }
            RowOutcome::Expired => self.expired_rows += 1,
            RowOutcome::NotDue => self.not_due_rows += 1,
            RowOutcome::Skipped | RowOutcome::Raced => self.skipped_rows += 1,
        }
    }
}

/// The LATEST rung whose `start_count × start_type` from `date_from` has
/// elapsed (HLB-17). Levels arrive sequence-ascending; walk from the top.
fn applicable_level(
    levels: &[AccrualLevelRow],
    date_from: chrono::NaiveDate,
    now: DateTime<Utc>,
) -> Option<&AccrualLevelRow> {
    let today = now.date_naive();
    levels
        .iter()
        .rev()
        .find(|l| match l.start_type.as_str() {
            "days" => date_from + Duration::days(l.start_count.to_i64().unwrap_or(i64::MAX)) <= today,
            "months" => add_months(date_from, l.start_count.to_i64().unwrap_or(i64::MAX)) <= today,
            "years" => add_months(date_from, 12 * l.start_count.to_i64().unwrap_or(i64::MAX)) <= today,
            other => {
                warn!(start_type = other, "accrual walk: unknown start_type — rung ignored");
                false
            }
        })
}

/// Headroom under `maximum_leave`; no cap → unbounded (Decimal::MAX).
fn headroom(level: &AccrualLevelRow, allocated: Decimal) -> Decimal {
    match level.maximum_leave {
        Some(cap) => (cap - allocated).max(Decimal::ZERO),
        None => Decimal::MAX,
    }
}

/// Over-cap disposal (HLB-18): `postponed_to_next_accrual` holds the excess in
/// `carried_over` bounded by `postponed_max_days` (the rest is lost);
/// `nothing` discards it all.
fn split_excess(level: &AccrualLevelRow, excess: Decimal) -> (Decimal, Decimal) {
    if excess.is_zero() {
        return (Decimal::ZERO, Decimal::ZERO);
    }
    match level.action.as_str() {
        "postponed_to_next_accrual" => {
            let bound = level.postponed_max_days.unwrap_or(excess);
            let carried = excess.min(bound).max(Decimal::ZERO);
            (carried, excess - carried)
        }
        _ => (Decimal::ZERO, excess),
    }
}

/// Whole frequency periods between watermark and now. Calendar-month counts
/// only COMPLETE months on the watermark's day-of-month (Odoo grants on the
/// allocation's anniversary day). `None` = unknown frequency spelling.
fn periods_due(frequency: &str, watermark: DateTime<Utc>, now: DateTime<Utc>) -> Option<i64> {
    let days = (now.date_naive() - watermark.date_naive()).num_days();
    Some(match frequency {
        "daily" => days,
        "weekly" => days / 7,
        "biweekly" => days / 14,
        "monthly" => whole_months(watermark.date_naive(), now.date_naive()),
        "bimonthly" => whole_months(watermark.date_naive(), now.date_naive()) / 2,
        "quarterly" => whole_months(watermark.date_naive(), now.date_naive()) / 3,
        "yearly" => whole_months(watermark.date_naive(), now.date_naive()) / 12,
        _ => return None,
    })
    .map(|p| p.max(0))
}

/// Complete calendar months between two dates: counts a month only once the
/// anniversary day is reached ((2026-01-15) → (2026-03-14) = 1, → (2026-03-15) = 2).
fn whole_months(from: chrono::NaiveDate, to: chrono::NaiveDate) -> i64 {
    let mut months = (to.year() as i64 - from.year() as i64) * 12 + (to.month() as i64 - from.month() as i64);
    if to.day() < from.day() {
        months -= 1;
    }
    months.max(0)
}

/// `from` + n calendar months, day clamped to the target month's length
/// (Jan 31 + 1 mo → Feb 28/29, chrono's checked_add_months semantics).
fn add_months(from: chrono::NaiveDate, months: i64) -> chrono::NaiveDate {
    from.checked_add_months(chrono::Months::new(months.max(0) as u32))
        .unwrap_or(from)
}

/// Watermark after `periods` granted periods: +days for week-family, +calendar
/// months (day-clamped) for the month family.
fn advance_watermark(frequency: &str, watermark: DateTime<Utc>, periods: i64) -> Option<DateTime<Utc>> {
    Some(match frequency {
        "daily" => watermark + Duration::days(periods),
        "weekly" => watermark + Duration::days(7 * periods),
        "biweekly" => watermark + Duration::days(14 * periods),
        "monthly" => swap_date(watermark, add_months(watermark.date_naive(), periods)),
        "bimonthly" => swap_date(watermark, add_months(watermark.date_naive(), 2 * periods)),
        "quarterly" => swap_date(watermark, add_months(watermark.date_naive(), 3 * periods)),
        "yearly" => swap_date(watermark, add_months(watermark.date_naive(), 12 * periods)),
        "once" => watermark,
        _ => return None,
    })
}

/// Swap a timestamp's calendar date, keeping its clock time (UTC).
fn swap_date(dt: DateTime<Utc>, d: NaiveDate) -> DateTime<Utc> {
    NaiveDateTime::new(
        d,
        NaiveTime::from_hms_opt(dt.hour(), dt.minute(), dt.second()).unwrap_or_default(),
    )
    .and_utc()
}
