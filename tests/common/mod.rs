//! Shared helpers for the timeoff module's behavior tests (user-owned).
//!
//! Live-pool pattern per the payroll/employee test convention: DATABASE_URL
//! wins, else the module's local test DB. Fresh random company ids per test so
//! parallel runs never collide.

#![allow(dead_code)]

use sqlx::PgPool;

pub fn dburl() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://serpa:serpa_dev_password@127.0.0.1:5432/backbone_timeoff_test".into()
    })
}

pub async fn pool() -> PgPool {
    PgPool::connect(&dburl()).await.expect("connect")
}
