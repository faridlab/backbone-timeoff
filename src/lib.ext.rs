//! The module's extension (hand-written; ADR-0031): what the module adds beside its
//! generated services. The generated module and builder carry `ModuleExt` and
//! `ModuleBuilderExt` and dereference to them, so their fields read as `module.field`.

#[allow(unused_imports)]
use super::*;

/// State the module adds; read through the generated module's `Deref`.
pub struct ModuleExt {
    // Held so the `TimeoffQueryService` impl can delegate `paid_leave_days` to the repo's
    // hand-written SQL, and standard lookups to the CRUD services above. `db_pool` is the same pool
    // the repo was constructed with (the repo's `paid_leave_days` takes it per the backbone-attendance
    // read-port convention — the ambient org scope the composing service bound fences it).
    pub(crate) timeoff_request_repository: Arc<TimeoffRequestRepository>,
    // The balance repo + write service are wired here so the leave-drawdown invariant is live on the
    // module, but no in-crate handler drives them yet (the approve/reject/cancel HTTP surface and a
    // public accessor land in the next phase, alongside the outbox/compound-event wiring per ADR-005).
    // `#[allow(dead_code)]` is honest about that — the service is fully functional, just not yet
    // called from inside this crate.
    #[allow(dead_code)]
    pub(crate) timeoff_balance_repository: Arc<TimeoffBalanceRepository>,
    #[allow(dead_code)]
    pub(crate) timeoff_write_service: Arc<application::service::TimeoffRequestWriteService>,
    pub(crate) db_pool: sqlx::PgPool,
}

/// State the builder adds.
pub struct ModuleBuilderExt {
}

impl Default for ModuleBuilderExt {
    fn default() -> Self {
        Self {
        }
    }
}

impl ModuleBuilderExt {
    /// Build the extension's state from what the generated build made.
    #[allow(unused_variables, clippy::redundant_clone)]
    pub(crate) fn build(self, parts: &ModuleParts<'_>) -> anyhow::Result<ModuleExt> {
        let db_pool = parts.db_pool.clone();
        let timeoff_balance_repository = parts.timeoff_balance_repository.clone();
        let timeoff_request_repository = parts.timeoff_request_repository.clone();
        // Retain the timeoff-request repo + pool on the module so the `TimeoffQueryService` impl can
        // delegate `paid_leave_days` (custom SQL on the repo) and supply the pool the repo method takes.
        // Construct the hand-authored leave-drawdown write service from the same pool — it owns the
        // approve/reject/cancel transitions and pairs them with the balance repo's draw/restore inside
        // one transaction (ported verbatim from backbone-hr's hr_write_service.rs).
        let timeoff_write_service = Arc::new(application::service::TimeoffRequestWriteService::new(db_pool.clone()));
        Ok(ModuleExt {
            timeoff_request_repository: timeoff_request_repository.clone(),
            timeoff_balance_repository: timeoff_balance_repository.clone(),
            timeoff_write_service,
            db_pool,
        })
    }
}

impl crate::ThisModule {
    /// The leave ADMIN surface: generic writes for the master data HR owns
    /// (types, accrual plans, accrual levels) — the read-only base plus these
    /// is what a real deployment mounts for policy management. Balances and
    /// requests stay OFF this surface: balances move through the audited
    /// adjust verb on the write service, requests through its lifecycle verbs.
    pub fn admin_routes(&self) -> Router {
        use presentation::http::{
            create_timeoff_accrual_level_write_routes, create_timeoff_accrual_plan_write_routes,
            create_timeoff_type_write_routes,
        };
        self.readonly_routes()
            .merge(create_timeoff_type_write_routes(
                self.timeoff_type_service.clone(),
            ))
            .merge(create_timeoff_accrual_plan_write_routes(
                self.timeoff_accrual_plan_service.clone(),
            ))
            .merge(create_timeoff_accrual_level_write_routes(
                self.timeoff_accrual_level_service.clone(),
            ))
    }
    /// The module-held validated write path (request verbs + the approvals seam).
    pub fn timeoff_write_service(&self) -> Arc<application::service::TimeoffRequestWriteService> {
        self.timeoff_write_service.clone()
    }
    /// Replace the write service at the composition root — the way a host arms the approvals
    /// seam: the module builds the service internally (default-unwired), so the app builds its
    /// own from the same pool with the port attached and swaps it in. Consuming `self` keeps
    /// the chain reading naturally; call before mounting routes, the module is consumed once
    /// at startup.
    ///
    /// ```text
    /// let timeoff = TimeoffModule::builder()
    ///     .with_database(pool.clone())
    ///     .build()?
    ///     .with_timeoff_write_service(Arc::new(
    ///         TimeoffRequestWriteService::new(pool.clone()).with_approvals(port),
    ///     ));
    /// ```
    pub fn with_timeoff_write_service(
        mut self,
        svc: Arc<application::service::TimeoffRequestWriteService>,
    ) -> Self {
        self.timeoff_write_service = svc;
        self
    }
}
