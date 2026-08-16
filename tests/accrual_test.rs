//! Behavior tests for the accrual engine + the approvals seam (Wave 1 P1, H-2).
//!
//! Live-pool pattern (payroll/employee convention): a migrated
//! `backbone_timeoff_test` DB on the metaphora dev postgres; fresh random
//! company ids per test. The walk tests serialize on a static lock — every
//! due row in the DB belongs to the lock-holder, so a run's outcome tallies
//! are exact and never process another test's mid-flight fixtures.
//!
//! Coverage map (spec: HLB-16/17/18, HLM-10, TR1–TR3):
//! - monthly grant math + watermark advance
//! - idempotency (re-run same period → NotDue; next period → only the delta)
//! - cap at maximum_leave + postponed carry (bounded) and its re-grant
//! - action_with_lost_days = nothing drops the excess
//! - validity-window expiry stamp
//! - the tenure ladder picks the LATEST eligible rung
//! - `once` grants exactly one time
//! - the approvals seam: wired port gates approve on the verdict (TR2);
//!   unwired keeps the pre-P1 behavior
//! - the draw gate honors the balance's validity window

mod common;

use chrono::{NaiveDate, TimeZone, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use backbone_timeoff::application::service::approvals_port::{
    ApprovalFiling, ApprovalFilingRequest, ApprovalVerdict,
};
use backbone_timeoff::application::service::{
    AccrualService, TimeoffError, TimeoffRequestWriteService,
};

/// Serializes the walk tests (see module doc).
static WALK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn at(y: i32, m: u32, d: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, 3, 0, 0).unwrap()
}

// ─── fixtures ─────────────────────────────────────────────────────────────────

async fn seed_type(pool: &PgPool, company_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO timeoff.timeoff_types (id, company_id, name, code)
           VALUES ($1, $2, 'Annual Leave', $3)"#,
    )
    .bind(id)
    .bind(company_id)
    .bind(format!("AL-{}", &id.to_string()[..8]))
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn seed_plan(pool: &PgPool, company_id: Uuid, type_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO timeoff.timeoff_accrual_plans (id, company_id, timeoff_type_id, name)
           VALUES ($1, $2, $3, 'Standard accrual')"#,
    )
    .bind(id)
    .bind(company_id)
    .bind(type_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[allow(clippy::too_many_arguments)]
async fn seed_level(
    pool: &PgPool,
    company_id: Uuid,
    plan_id: Uuid,
    sequence: i32,
    start_count: Decimal,
    start_type: &str,
    frequency: &str,
    added_value: Decimal,
    maximum_leave: Option<Decimal>,
    action: &str,
    postponed_max_days: Option<Decimal>,
) {
    sqlx::query(
        r#"INSERT INTO timeoff.timeoff_accrual_levels
               (id, company_id, plan_id, sequence, start_count, start_type,
                frequency, added_value, is_added_based_on_worked_time,
                maximum_leave, action_with_lost_days, postponed_max_days)
           VALUES ($1, $2, $3, $4, $5, $6::accrual_start_type, $7::accrual_frequency,
                   $8, false, $9, $10::accrual_lost_days_action, $11)"#,
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(plan_id)
    .bind(sequence)
    .bind(start_count)
    .bind(start_type)
    .bind(frequency)
    .bind(added_value)
    .bind(maximum_leave)
    .bind(action)
    .bind(postponed_max_days)
    .execute(pool)
    .await
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn seed_balance(
    pool: &PgPool,
    company_id: Uuid,
    type_id: Uuid,
    employee_id: Uuid,
    period: &str,
    allocated: Decimal,
    plan_id: Option<Uuid>,
    date_from: Option<NaiveDate>,
    date_to: Option<NaiveDate>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO timeoff.timeoff_balances
               (id, company_id, timeoff_type_id, employee_id, period,
                allocated, used, accrual_plan_id, date_from, date_to)
           VALUES ($1, $2, $3, $4, $5, $6, 0, $7, $8, $9)"#,
    )
    .bind(id)
    .bind(company_id)
    .bind(type_id)
    .bind(employee_id)
    .bind(period)
    .bind(allocated)
    .bind(plan_id)
    .bind(date_from)
    .bind(date_to)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// (allocated, used, carried_over, last_accrual_at, expired_at) for one balance.
async fn balance_state(pool: &PgPool, id: Uuid) -> (Decimal, Decimal, Decimal, Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>) {
    sqlx::query_as::<_, (Decimal, Decimal, Decimal, Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>)>(
        r#"SELECT allocated, used, carried_over, last_accrual_at, expired_at
           FROM timeoff.timeoff_balances WHERE id = $1"#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ─── the engine ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn accrual_monthly_grant_advances_watermark() {
    let _g = WALK.lock().await;
    let pool = common::pool().await;
    let company = Uuid::new_v4();
    let type_id = seed_type(&pool, company).await;
    let plan = seed_plan(&pool, company, type_id).await;
    seed_level(&pool, company, plan, 1, Decimal::ZERO, "days", "monthly",
               Decimal::new(15, 1), None, "nothing", None).await;
    let bal = seed_balance(&pool, company, type_id, Uuid::new_v4(), "2026",
                           Decimal::ZERO, Some(plan), Some(day(2026, 1, 1)), None).await;

    // 2026-01-01 → 2026-03-10: TWO whole monthly periods elapsed (Feb 1, Mar 1).
    AccrualService::new(pool.clone()).run_accrual(at(2026, 3, 10), 50, 10).await.unwrap();
    let (allocated, _, _, watermark, expired) = balance_state(&pool, bal).await;
    assert_eq!(allocated, Decimal::from(3));
    // The watermark advances by whole periods from the midnight-of-date_from
    // anniversary, NOT to `now` — remainders are never silently absorbed.
    assert_eq!(watermark, Some(Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap()));
    assert!(expired.is_none());
}

#[tokio::test]
async fn accrual_is_idempotent_and_grants_only_the_delta() {
    let _g = WALK.lock().await;
    let pool = common::pool().await;
    let company = Uuid::new_v4();
    let type_id = seed_type(&pool, company).await;
    let plan = seed_plan(&pool, company, type_id).await;
    seed_level(&pool, company, plan, 1, Decimal::ZERO, "days", "monthly",
               Decimal::new(15, 1), None, "nothing", None).await;
    let bal = seed_balance(&pool, company, type_id, Uuid::new_v4(), "2026",
                           Decimal::ZERO, Some(plan), Some(day(2026, 1, 1)), None).await;

    let svc = AccrualService::new(pool.clone());
    svc.run_accrual(at(2026, 3, 10), 50, 10).await.unwrap();
    // Same period re-run: nothing double-grants (the watermark guard — the
    // state assert below is the proof; the run-level tally may include other
    // tests' rows that are legitimately due at this `now`).
    let _ = svc.run_accrual(at(2026, 3, 11), 50, 10).await.unwrap();
    // Next period: exactly one more period's delta.
    svc.run_accrual(at(2026, 4, 5), 50, 10).await.unwrap();
    let (allocated, _, _, watermark, _) = balance_state(&pool, bal).await;
    assert_eq!(allocated, Decimal::new(45, 1)); // 4.5 = 3 periods + 1
    assert_eq!(watermark, Some(Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap()));
}

#[tokio::test]
async fn accrual_caps_at_maximum_and_postpones_the_excess() {
    let _g = WALK.lock().await;
    let pool = common::pool().await;
    let company = Uuid::new_v4();
    let type_id = seed_type(&pool, company).await;
    let plan = seed_plan(&pool, company, type_id).await;
    // 2.0/month, cap 5, postpone up to 3.
    seed_level(&pool, company, plan, 1, Decimal::ZERO, "days", "monthly",
               Decimal::from(2), Some(Decimal::from(5)),
               "postponed_to_next_accrual", Some(Decimal::from(3))).await;
    let bal = seed_balance(&pool, company, type_id, Uuid::new_v4(), "2026",
                           Decimal::from(4), Some(plan), Some(day(2026, 1, 1)), None).await;

    let svc = AccrualService::new(pool.clone());
    // 2 periods → gross 4, headroom 1: grant 1, postpone 3.
    svc.run_accrual(at(2026, 3, 10), 50, 10).await.unwrap();
    let (allocated, _, carried, _, _) = balance_state(&pool, bal).await;
    assert_eq!(allocated, Decimal::from(5));
    assert_eq!(carried, Decimal::from(3));

    // Next period: gross = 2 (grant) + 3 (released carry) = 5, headroom 0 →
    // granted 0, excess 5 → postpone min(5, cap 3) = 3, lose 2.
    let out = svc.run_accrual(at(2026, 4, 10), 50, 10).await.unwrap();
    assert!(out.lost_days >= Decimal::from(2));
    let (allocated, _, carried, _, _) = balance_state(&pool, bal).await;
    assert_eq!(allocated, Decimal::from(5));
    assert_eq!(carried, Decimal::from(3));
}

#[tokio::test]
async fn accrual_lost_days_action_nothing_drops_the_excess() {
    let _g = WALK.lock().await;
    let pool = common::pool().await;
    let company = Uuid::new_v4();
    let type_id = seed_type(&pool, company).await;
    let plan = seed_plan(&pool, company, type_id).await;
    seed_level(&pool, company, plan, 1, Decimal::ZERO, "days", "monthly",
               Decimal::from(2), Some(Decimal::from(5)), "nothing", None).await;
    let bal = seed_balance(&pool, company, type_id, Uuid::new_v4(), "2026",
                           Decimal::from(4), Some(plan), Some(day(2026, 1, 1)), None).await;

    let out = AccrualService::new(pool.clone())
        .run_accrual(at(2026, 3, 10), 50, 10).await.unwrap();
    assert!(out.lost_days >= Decimal::from(3));
    assert_eq!(out.postponed_days, Decimal::ZERO);
    let (allocated, _, carried, _, _) = balance_state(&pool, bal).await;
    assert_eq!(allocated, Decimal::from(5));
    assert_eq!(carried, Decimal::ZERO);
}

#[tokio::test]
async fn accrual_expires_past_the_validity_window() {
    let _g = WALK.lock().await;
    let pool = common::pool().await;
    let company = Uuid::new_v4();
    let type_id = seed_type(&pool, company).await;
    let plan = seed_plan(&pool, company, type_id).await;
    seed_level(&pool, company, plan, 1, Decimal::ZERO, "days", "monthly",
               Decimal::new(15, 1), None, "nothing", None).await;
    let bal = seed_balance(&pool, company, type_id, Uuid::new_v4(), "2026",
                           Decimal::ZERO, Some(plan), Some(day(2026, 1, 1)), Some(day(2026, 2, 1))).await;

    AccrualService::new(pool.clone()).run_accrual(at(2026, 3, 10), 50, 10).await.unwrap();
    let (allocated, _, _, _, expired) = balance_state(&pool, bal).await;
    assert_eq!(allocated, Decimal::ZERO); // expired balances stop accruing
    assert!(expired.is_some());
}

#[tokio::test]
async fn accrual_ladder_picks_the_latest_eligible_rung() {
    let _g = WALK.lock().await;
    let pool = common::pool().await;
    let company = Uuid::new_v4();
    let type_id = seed_type(&pool, company).await;
    let plan = seed_plan(&pool, company, type_id).await;
    // Year 1: 1.0/month. After 12 months of tenure: 2.0/month.
    seed_level(&pool, company, plan, 1, Decimal::ZERO, "days", "monthly",
               Decimal::from(1), None, "nothing", None).await;
    seed_level(&pool, company, plan, 2, Decimal::from(12), "months", "monthly",
               Decimal::from(2), None, "nothing", None).await;
    let bal = seed_balance(&pool, company, type_id, Uuid::new_v4(), "2026",
                           Decimal::ZERO, Some(plan), Some(day(2025, 1, 1)), None).await;

    // 14 whole months elapsed since 2025-01-01 → the 12-month rung applies.
    AccrualService::new(pool.clone()).run_accrual(at(2026, 3, 10), 50, 10).await.unwrap();
    let (allocated, _, _, _, _) = balance_state(&pool, bal).await;
    assert_eq!(allocated, Decimal::from(28)); // 14 periods × 2.0
}

#[tokio::test]
async fn accrual_once_grants_exactly_one_time() {
    let _g = WALK.lock().await;
    let pool = common::pool().await;
    let company = Uuid::new_v4();
    let type_id = seed_type(&pool, company).await;
    let plan = seed_plan(&pool, company, type_id).await;
    seed_level(&pool, company, plan, 1, Decimal::ZERO, "days", "once",
               Decimal::from(5), None, "nothing", None).await;
    let bal = seed_balance(&pool, company, type_id, Uuid::new_v4(), "2026",
                           Decimal::ZERO, Some(plan), Some(day(2026, 1, 1)), None).await;

    let svc = AccrualService::new(pool.clone());
    svc.run_accrual(at(2026, 1, 15), 50, 10).await.unwrap();
    let (allocated, _, _, _, _) = balance_state(&pool, bal).await;
    assert_eq!(allocated, Decimal::from(5));
    let _ = svc.run_accrual(at(2026, 6, 15), 50, 10).await.unwrap();
    let (allocated, _, _, _, _) = balance_state(&pool, bal).await;
    assert_eq!(allocated, Decimal::from(5), "once must never re-grant");
}

// ─── the approvals seam + the draw window ─────────────────────────────────────

/// The in-test approvals engine: records filings, replays a mutable verdict map.
struct FakeApprovals {
    filings: Mutex<Vec<ApprovalFilingRequest>>,
    verdicts: Mutex<HashMap<Uuid, ApprovalVerdict>>,
}

impl FakeApprovals {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            filings: Mutex::new(Vec::new()),
            verdicts: Mutex::new(HashMap::new()),
        })
    }

    fn set_verdict(&self, approval_request_id: Uuid, verdict: ApprovalVerdict) {
        self.verdicts.lock().unwrap().insert(approval_request_id, verdict);
    }
}

#[async_trait::async_trait]
impl ApprovalFiling for FakeApprovals {
    async fn file(&self, req: &ApprovalFilingRequest) -> Result<Uuid, backbone_timeoff::application::service::approvals_port::ApprovalSeamError> {
        let id = Uuid::new_v4();
        self.filings.lock().unwrap().push(req.clone());
        self.verdicts.lock().unwrap().insert(id, ApprovalVerdict::Pending);
        Ok(id)
    }

    async fn status(&self, approval_request_id: Uuid) -> Result<ApprovalVerdict, backbone_timeoff::application::service::approvals_port::ApprovalSeamError> {
        self.verdicts
            .lock()
            .unwrap()
            .get(&approval_request_id)
            .copied()
            .ok_or(backbone_timeoff::application::service::approvals_port::ApprovalSeamError::UnknownApprovalRequest(approval_request_id))
    }
}

#[tokio::test]
async fn seam_wired_submits_file_and_approve_honors_the_verdict() {
    let pool = common::pool().await;
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let type_id = seed_type(&pool, company).await;
    seed_balance(&pool, company, type_id, employee, "2026", Decimal::from(5), None, None, None).await;

    let port = FakeApprovals::new();
    let svc = TimeoffRequestWriteService::new(pool.clone()).with_approvals(port.clone());

    let request_id = svc
        .submit_request(company, type_id, employee, day(2026, 3, 2), day(2026, 3, 3), Some("family event".into()))
        .await
        .unwrap();

    // Filed with the engine, linked on the row, with the inclusive day count.
    {
        let filings = port.filings.lock().unwrap();
        assert_eq!(filings.len(), 1);
        assert_eq!(filings[0].timeoff_request_id, request_id);
        assert_eq!(filings[0].days, Decimal::from(2));
    }
    let linked: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT approval_request_id FROM timeoff.timeoff_requests WHERE id = $1"#,
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let approval_id = linked.expect("request must be linked to its filing");

    // TR2: pending verdict blocks the approve.
    let blocked = svc.approve_request(request_id, None).await.unwrap_err();
    assert!(matches!(blocked, TimeoffError::ApprovalNotGranted));

    // Engine grants → the verb passes (and draws the balance).
    port.set_verdict(approval_id, ApprovalVerdict::Approved);
    svc.approve_request(request_id, None).await.unwrap();
    let status: String = sqlx::query_scalar(
        r#"SELECT status::text FROM timeoff.timeoff_requests WHERE id = $1"#,
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "approved");
}

#[tokio::test]
async fn seam_unwired_keeps_the_pre_p1_behavior() {
    let pool = common::pool().await;
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let type_id = seed_type(&pool, company).await;
    seed_balance(&pool, company, type_id, employee, "2026", Decimal::from(5), None, None, None).await;

    let svc = TimeoffRequestWriteService::new(pool.clone());
    let request_id = svc
        .submit_request(company, type_id, employee, day(2026, 3, 2), day(2026, 3, 3), None)
        .await
        .unwrap();

    let linked: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT approval_request_id FROM timeoff.timeoff_requests WHERE id = $1"#,
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(linked.is_none(), "unwired seam must not link a filing");

    // Direct manager approval still works (backward compatible).
    svc.approve_request(request_id, None).await.unwrap();
}

#[tokio::test]
async fn draw_gate_honors_the_balance_validity_window() {
    let pool = common::pool().await;
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let type_id = seed_type(&pool, company).await;
    // Allocation valid June 2026 only.
    seed_balance(&pool, company, type_id, employee, "2026", Decimal::from(5), None,
                 Some(day(2026, 6, 1)), Some(day(2026, 6, 30))).await;

    let svc = TimeoffRequestWriteService::new(pool.clone());

    // July leave cannot draw from a June-only allocation.
    let outside = svc
        .submit_request(company, type_id, employee, day(2026, 7, 1), day(2026, 7, 2), None)
        .await
        .unwrap();
    let refused = svc.approve_request(outside, None).await.unwrap_err();
    assert!(matches!(refused, TimeoffError::InsufficientBalance));

    // June leave draws fine.
    let inside = svc
        .submit_request(company, type_id, employee, day(2026, 6, 1), day(2026, 6, 2), None)
        .await
        .unwrap();
    svc.approve_request(inside, None).await.unwrap();
}

#[tokio::test]
async fn fence_policies_cover_every_fenced_table() {
    let pool = common::pool().await;
    let policies: Vec<String> = sqlx::query_scalar(
        r#"SELECT policyname FROM pg_policies WHERE schemaname = 'timeoff' ORDER BY 1"#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for expected in [
        "timeoff_types_company_isolation",
        "timeoff_requests_company_isolation",
        "timeoff_balances_company_isolation",
        "timeoff_accrual_plans_company_isolation",
        "timeoff_accrual_levels_company_isolation",
    ] {
        assert!(policies.iter().any(|p| p == expected), "missing fence policy {expected}");
    }
}
