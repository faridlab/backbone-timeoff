//! What an approved request drew is what its cancel gives back: a generic write cannot change the
//! terms an approval drew for, and a cancel whose balance can no longer cover the days refuses with
//! the declared code. Requires DATABASE_URL (a migrated scratch DB).

mod common;

use chrono::NaiveDate;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use backbone_timeoff::application::service::{build_timeoff_request_service, TimeoffRequestWriteService};
use backbone_timeoff::application::service::timeoff_request_service_custom::TimeoffError;
use backbone_timeoff::TimeoffRequestRepository;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

async fn seed(pool: &sqlx::PgPool, employee: Uuid) -> Uuid {
    let type_id = Uuid::new_v4();
    sqlx::query("INSERT INTO timeoff.timeoff_types (id, name, code) VALUES ($1, 'Annual', $2)")
        .bind(type_id).bind(format!("AL-{}", &type_id.simple().to_string()[..6]))
        .execute(pool).await.unwrap();
    sqlx::query(
        "INSERT INTO timeoff.timeoff_balances (id, timeoff_type_id, employee_id, period, allocated, used) \
         VALUES ($1, $2, $3, '2026', 30, 0)",
    )
    .bind(Uuid::new_v4()).bind(type_id).bind(employee)
    .execute(pool).await.unwrap();
    type_id
}

#[tokio::test]
async fn a_generic_write_cannot_change_what_an_approval_drew_for() {
    let pool = common::pool().await;
    common::scoped_as(&pool, Uuid::new_v4(), async {
        let employee = Uuid::new_v4();
        let type_id = seed(&pool, employee).await;
        let svc = TimeoffRequestWriteService::new(pool.clone());
        let req = svc.submit_request(type_id, employee, day(2026, 3, 2), day(2026, 3, 3), None).await.unwrap();
        svc.approve_request(req, None).await.unwrap();

        let generic = build_timeoff_request_service(Arc::new(TimeoffRequestRepository::new(pool.clone())));
        let mut longer = HashMap::new();
        longer.insert("date_end".to_string(), serde_json::json!("2026-03-10"));
        let refused = generic.partial_update(&req.to_string(), longer).await;
        let text = format!("{refused:?}");
        assert!(refused.is_err() && text.contains("approved_request_terms_frozen"), "{text}");
    })
    .await;
}

#[tokio::test]
async fn a_cancel_that_cannot_give_the_days_back_refuses_with_its_code() {
    let pool = common::pool().await;
    common::scoped_as(&pool, Uuid::new_v4(), async {
        let employee = Uuid::new_v4();
        let type_id = seed(&pool, employee).await;
        let svc = TimeoffRequestWriteService::new(pool.clone());
        let req = svc.submit_request(type_id, employee, day(2026, 3, 2), day(2026, 3, 3), None).await.unwrap();
        svc.approve_request(req, None).await.unwrap();
        // Another writer has already given the days back.
        sqlx::query("UPDATE timeoff.timeoff_balances SET used = 0 WHERE employee_id = $1")
            .bind(employee).execute(&pool).await.unwrap();

        let refused = svc.cancel_request(req).await;
        assert!(matches!(refused, Err(TimeoffError::CannotRestoreBalance)), "{refused:?}");
    })
    .await;
}
