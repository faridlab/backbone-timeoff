//! Shared helpers for the timeoff module's behavior tests (user-owned).
//!
//! Live-pool pattern per the payroll/employee test convention: DATABASE_URL
//! wins, else the module's local test DB. Fresh random primary keys per test
//! so parallel runs never collide.

#![allow(dead_code)]

use sqlx::PgPool;
use uuid::Uuid;

pub fn dburl() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://serpa:serpa_dev_password@127.0.0.1:5432/backbone_timeoff_test".into()
    })
}

pub async fn pool() -> PgPool {
    PgPool::connect(&dburl()).await.expect("connect")
}

/// Bind a request org scope around `f` — the composing-service posture the write
/// verbs need: their company-keyed outbound twins (approvals filing, settled-leave
/// events) read the ambient scope's legacy company id, fail-closed. `unit` doubles
/// as that legacy company id.
pub async fn scoped_as<R, F>(pool: &PgPool, unit: Uuid, f: F) -> R
where
    F: std::future::Future<Output = R>,
{
    backbone_orm::org_scope::with_org_request_scope(
        pool,
        backbone_orm::org_scope::OrgScope::for_company_unit(unit),
        f,
    )
    .await
    .expect("bind org request scope")
}
