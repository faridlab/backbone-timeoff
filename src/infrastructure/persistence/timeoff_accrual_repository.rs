//! The accrual walk's SQL (Wave 1 P1, H-2) — user-owned (declared under
//! `user_owned` in `metaphor.codegen.yaml`).
//!
//! Ported from Odoo `hr.leave.allocation`'s
//! `_compute_accrual` / `hr.leave.accrual.level._get_applicable_level` family
//! (HLB-16/17/18, HLM-10). Repos hold the SQL (4-layer rule); the period math
//! and cap/carry policy live in `application::service::accrual_service`.
//!
//! Concurrency (ADR-0020, `pickup_lock`): the claim is `FOR UPDATE SKIP LOCKED`
//! inside a SHORT claim tx — two concurrent runs split the candidate set
//! instead of double-walking it. Because the claim commits before any grants
//! (`commit_per_batch`), the lock alone would not stop a later re-claim of the
//! same rows; every apply is therefore additionally guarded by an optimistic
//! watermark check (`last_accrual_at IS NOT DISTINCT FROM` the value read at
//! claim time), so a row another walker already advanced matches zero rows and
//! is skipped. The grant itself can never double-fire.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::FromRow;
use uuid::Uuid;

/// A balance claimed for the walk, with the state the engine needs. The enums
/// come back as text (EPP-2 lesson: raw-SQL enum decode into Rust Strings;
/// the engine matches on `&str`).
#[derive(Debug, Clone, FromRow)]
pub struct AccrualWalkRow {
    pub id: Uuid,
    pub timeoff_type_id: Uuid,
    pub employee_id: Uuid,
    pub period: String,
    pub allocated: Decimal,
    pub used: Decimal,
    pub accrual_plan_id: Uuid,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub last_accrual_at: Option<DateTime<Utc>>,
    pub carried_over: Decimal,
}

/// One rung of a plan's ladder. `frequency`/`start_type`/`action` are the PG
/// enum values as text.
#[derive(Debug, Clone, FromRow)]
pub struct AccrualLevelRow {
    pub id: Uuid,
    pub sequence: i32,
    pub start_count: Decimal,
    pub start_type: String,
    pub frequency: String,
    pub added_value: Decimal,
    pub is_added_based_on_worked_time: bool,
    pub maximum_leave: Option<Decimal>,
    pub action: String,
    pub postponed_max_days: Option<Decimal>,
}

/// Walk SQL for the accrual engine. Not a generic CRUD repo — a purpose-built
/// reader/writer pair for the daily cron (`scheduled_jobs.accrual_update`,
/// posture `self_arming`).
#[derive(Clone)]
pub struct TimeoffAccrualRepository {
    pool: sqlx::PgPool,
}

impl TimeoffAccrualRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Claim up to `batch` candidate balances for one walk pass. Candidates =
    /// plan-linked, not yet expired, not soft-deleted. Rows past `date_to` are
    /// INCLUDED — the walk stamps their expiry (spec step 5). The SKIP LOCKED
    /// claim runs in the caller's short tx; commit it before applying grants.
    pub async fn claim_batch_for_walk(
        &self,
        conn: &mut sqlx::PgConnection,
        batch: i64,
    ) -> Result<Vec<AccrualWalkRow>, sqlx::Error> {
        sqlx::query_as::<_, AccrualWalkRow>(
            r#"SELECT id, timeoff_type_id, employee_id, period,
                      allocated, used, accrual_plan_id, date_from, date_to,
                      last_accrual_at, carried_over
               FROM timeoff.timeoff_balances
               WHERE accrual_plan_id IS NOT NULL
                 AND expired_at IS NULL
                 AND (metadata->>'deleted_at') IS NULL
               ORDER BY date_from NULLS LAST, id
               LIMIT $1
               FOR UPDATE SKIP LOCKED"#,
        )
        .bind(batch)
        .fetch_all(conn)
        .await
    }

    /// The plan's ladder, low sequence first (the walk picks the LATEST rung
    /// whose start offset has elapsed — spec HLB-17).
    pub async fn find_levels_for_plan(
        &self,
        conn: &mut sqlx::PgConnection,
        plan_id: Uuid,
    ) -> Result<Vec<AccrualLevelRow>, sqlx::Error> {
        sqlx::query_as::<_, AccrualLevelRow>(
            r#"SELECT id, sequence, start_count, start_type::text AS start_type,
                      frequency::text AS frequency, added_value,
                      is_added_based_on_worked_time, maximum_leave,
                      action_with_lost_days::text AS action, postponed_max_days
               FROM timeoff.timeoff_accrual_levels
               WHERE plan_id = $1 AND (metadata->>'deleted_at') IS NULL
               ORDER BY sequence"#,
        )
        .bind(plan_id)
        .fetch_all(conn)
        .await
    }

    /// Apply one grant. Optimistic-guarded on the watermark read at claim time
    /// and on `expired_at IS NULL`; returns 0 when another walker already
    /// moved the row (skip silently — its grant is as good as ours) or when
    /// the expiry stamp raced us (correct: expired balances stop accruing).
    ///
    /// `granted` is ADDED to `allocated` (never negative — the engine caps at
    /// 0 headroom); `carried_over` is SET (the engine owns the full carry
    /// math, always ≥ 0); `new_watermark` is the advanced `last_accrual_at`
    /// (whole periods past the old watermark, not `now`, so remainders are
    /// never silently absorbed).
    pub async fn apply_grant(
        &self,
        conn: &mut sqlx::PgConnection,
        balance_id: Uuid,
        expected_watermark: Option<DateTime<Utc>>,
        granted: Decimal,
        carried_over: Decimal,
        new_watermark: DateTime<Utc>,
    ) -> Result<u64, sqlx::Error> {
        let done = sqlx::query(
            r#"UPDATE timeoff.timeoff_balances
               SET allocated = allocated + $3,
                   carried_over = $4,
                   last_accrual_at = $5
               WHERE id = $1
                 AND last_accrual_at IS NOT DISTINCT FROM $2
                 AND expired_at IS NULL"#,
        )
        .bind(balance_id)
        .bind(expected_watermark)
        .bind(granted)
        .bind(carried_over)
        .bind(new_watermark)
        .execute(conn)
        .await?;
        Ok(done.rows_affected())
    }

    /// Stamp the validity-window expiry (spec HLB-18 step 5: past `date_to`,
    /// the balance expires). Guarded so a double stamp is a no-op.
    pub async fn stamp_expired(
        &self,
        conn: &mut sqlx::PgConnection,
        balance_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<u64, sqlx::Error> {
        let done = sqlx::query(
            r#"UPDATE timeoff.timeoff_balances
               SET expired_at = $2
               WHERE id = $1 AND expired_at IS NULL"#,
        )
        .bind(balance_id)
        .bind(now)
        .execute(conn)
        .await?;
        Ok(done.rows_affected())
    }

    /// Read one balance back (engine tests + the walk's own assertions).
    pub async fn find_balance(
        &self,
        balance_id: Uuid,
    ) -> Result<Option<AccrualWalkRow>, sqlx::Error> {
        sqlx::query_as::<_, AccrualWalkRow>(
            r#"SELECT id, timeoff_type_id, employee_id, period,
                      allocated, used, accrual_plan_id, date_from, date_to,
                      last_accrual_at, carried_over
               FROM timeoff.timeoff_balances
               WHERE id = $1"#,
        )
        .bind(balance_id)
        .fetch_optional(&self.pool)
        .await
    }
}
