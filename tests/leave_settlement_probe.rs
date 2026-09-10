//! Leave-settlement event probes: the write path's outbound seam.
//!
//! Live-pool pattern (payroll/employee convention): a migrated scratch DB via DATABASE_URL;
//! fresh random primary keys per test so parallel runs never collide.
//!
//! Coverage map:
//! - every settling verb publishes exactly one `LeaveSettled` AFTER its commit, with the full
//!   payload (legacy company twin, request, employee, window)
//! - approve of a zero-day window settles `Voided` (granted, but no absence to project)
//! - cancelling an approved request settles `Cancelled` (after the balance restore commits)
//! - failed verbs (wrong state) emit nothing
//! - the default (unwired) sink keeps verb behavior unchanged

mod common;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use backbone_timeoff::application::service::timeoff_events::{LeaveSettlement, TimeoffEvent};
use backbone_timeoff::application::service::{
    TimeoffEventSink, TimeoffRequestWriteService,
};

/// Records every event handed to it — the capturing double for these probes.
#[derive(Clone, Default)]
struct CapturingSink(Arc<Mutex<Vec<TimeoffEvent>>>);

impl CapturingSink {
    fn events(&self) -> Vec<TimeoffEvent> {
        self.0.lock().unwrap().clone()
    }
}

impl TimeoffEventSink for CapturingSink {
    fn publish(&self, event: &TimeoffEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

async fn seed_type(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO timeoff.timeoff_types (id, name, code)
           VALUES ($1, 'Annual Leave', $2)"#,
    )
    .bind(id)
    .bind(format!("AL-{}", &id.to_string()[..8]))
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn seed_balance(pool: &PgPool, type_id: Uuid, employee_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO timeoff.timeoff_balances
               (id, timeoff_type_id, employee_id, period, allocated, used)
           VALUES ($1, $2, $3, '2026', 30, 0)"#,
    )
    .bind(Uuid::new_v4())
    .bind(type_id)
    .bind(employee_id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn to_sink_emits_on_approve_refuse_cancel_void() {
    let pool = common::pool().await;
    // The unit the ambient scope resolves to — the legacy company twin the settled
    // events carry (sourced from the scope, fail-closed).
    let company = Uuid::new_v4();
    common::scoped_as(&pool, company, async {
        let employee = Uuid::new_v4();
        let type_id = seed_type(&pool).await;
        seed_balance(&pool, type_id, employee).await;

        let sink = CapturingSink::default();
        let svc = TimeoffRequestWriteService::new(pool.clone()).with_events(Arc::new(sink.clone()));

        // ── approve ──────────────────────────────────────────────────────────────
        let req = svc
            .submit_request(type_id, employee, day(2026, 3, 2), day(2026, 3, 3), None)
            .await
            .unwrap();
        svc.approve_request(req, None).await.unwrap();
        let events = sink.events();
        assert_eq!(events.len(), 1, "approve emits exactly one event");
        match &events[0] {
            TimeoffEvent::LeaveSettled(e) => {
                assert_eq!(e.settlement, LeaveSettlement::Approved);
                assert_eq!(e.company_id, company);
                assert_eq!(e.request_id, req);
                assert_eq!(e.employee_id, employee);
                assert_eq!(e.date_from, day(2026, 3, 2));
                assert_eq!(e.date_to, day(2026, 3, 3));
            }
        }

        // ── refuse ───────────────────────────────────────────────────────────────
        let req = svc
            .submit_request(type_id, employee, day(2026, 4, 6), day(2026, 4, 7), None)
            .await
            .unwrap();
        svc.reject_request(req).await.unwrap();
        let events = sink.events();
        assert_eq!(events.len(), 2, "refuse appends one event");
        match &events[1] {
            TimeoffEvent::LeaveSettled(e) => {
                assert_eq!(e.settlement, LeaveSettlement::Refused);
                assert_eq!(e.request_id, req);
                assert_eq!(e.date_from, day(2026, 4, 6));
                assert_eq!(e.date_to, day(2026, 4, 7));
            }
        }

        // ── cancel a pending request ────────────────────────────────────────────
        let req = svc
            .submit_request(type_id, employee, day(2026, 5, 4), day(2026, 5, 5), None)
            .await
            .unwrap();
        svc.cancel_request(req).await.unwrap();
        let events = sink.events();
        assert_eq!(events.len(), 3, "pending-cancel appends one event");
        match &events[2] {
            TimeoffEvent::LeaveSettled(e) => {
                assert_eq!(e.settlement, LeaveSettlement::Cancelled);
                assert_eq!(e.request_id, req);
            }
        }

        // ── cancel an approved request (after the restore commits) ──────────────
        let req = svc
            .submit_request(type_id, employee, day(2026, 6, 1), day(2026, 6, 2), None)
            .await
            .unwrap();
        svc.approve_request(req, None).await.unwrap();
        svc.cancel_request(req).await.unwrap();
        let events = sink.events();
        assert_eq!(events.len(), 5, "approve + approved-cancel append two events");
        match &events[4] {
            TimeoffEvent::LeaveSettled(e) => {
                assert_eq!(e.settlement, LeaveSettlement::Cancelled);
                assert_eq!(e.request_id, req);
            }
        }

        // ── zero-day window settles Voided ──────────────────────────────────────
        let req = svc
            .submit_request(type_id, employee, day(2026, 7, 10), day(2026, 7, 9), None)
            .await
            .unwrap();
        svc.approve_request(req, None).await.unwrap();
        let events = sink.events();
        assert_eq!(events.len(), 6, "void-settle appends one event");
        match &events[5] {
            TimeoffEvent::LeaveSettled(e) => {
                assert_eq!(e.settlement, LeaveSettlement::Voided);
                assert_eq!(e.request_id, req);
            }
        }

        // ── failed verbs emit nothing ────────────────────────────────────────────
        // req above is now approved: re-approving fails, and no event lands.
        svc.approve_request(req, None).await.unwrap_err();
        svc.reject_request(req).await.unwrap_err();
        assert_eq!(sink.events().len(), 6, "failed verbs never emit");
    })
    .await;
}

#[tokio::test]
async fn to_default_sink_keeps_verb_behavior() {
    let pool = common::pool().await;
    common::scoped_as(&pool, Uuid::new_v4(), async {
        let employee = Uuid::new_v4();
        let type_id = seed_type(&pool).await;
        seed_balance(&pool, type_id, employee).await;

        // No with_events: the default LoggingSink swallows events; the verbs behave as before.
        let svc = TimeoffRequestWriteService::new(pool.clone());
        let req = svc
            .submit_request(type_id, employee, day(2026, 3, 2), day(2026, 3, 3), None)
            .await
            .unwrap();
        svc.approve_request(req, None).await.unwrap();
        svc.cancel_request(req).await.unwrap();

        let (used,): (Decimal,) = sqlx::query_as(
            "SELECT used FROM timeoff.timeoff_balances \
             WHERE timeoff_type_id = $1 AND employee_id = $2 AND period = '2026'",
        )
        .bind(type_id)
        .bind(employee)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(used, Decimal::ZERO, "approve drew 2 days, cancel restored them");
    })
    .await;
}
