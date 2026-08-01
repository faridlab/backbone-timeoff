//! The hand-authored TimeoffRequest write path (user-owned; survives regen via the `*_custom.rs`
//! suffix AND is listed under `user_owned` in `metaphor.codegen.yaml`).
//!
//! The leave-balance engine. Posts NO GL. The load-bearing invariant is the **timeoff balance**:
//! approving a request draws down the employee's balance for that timeoff type/period, gated so
//! `used` never exceeds `allocated` (you cannot approve leave you don't have), and cancelling an
//! approved request restores it — the draw + the request transition commit in ONE transaction.
//!
//! Ported verbatim from backbone-hr's `hr_write_service.rs` (`approve_leave` / `reject_leave` /
//! `cancel_leave`). The schema-driven adaptations are documented inline; the tx + gating + rollback
//! logic is identical. The one deferral: backbone-hr emits `LeaveApproved` via an `HrEventSink`;
//! real event emission here is deferred to the outbox/compound-event phase per ADR-005 — see the
//! TODO at each commit point.

use backbone_orm::company_scope;
use chrono::Datelike;
use sqlx::PgPool;
use uuid::Uuid;

use crate::infrastructure::persistence::{
    TimeoffBalanceRepository, TimeoffRequestRepository,
};

#[derive(Debug, thiserror::Error)]
pub enum TimeoffError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("not found: {0}")]
    NotFound(&'static str),
    #[error("invalid state: {0}")]
    InvalidState(&'static str),
    #[error("insufficient timeoff balance")]
    InsufficientBalance,
}

/// The hand-authored TimeoffRequest write service — owns the leave-drawdown invariant.
///
/// Mirrors backbone-hr's `HrWriteService` (the leave half). Holds the pool plus the two repositories
/// the invariant spans: `TimeoffRequestRepository` (the transition) and `TimeoffBalanceRepository`
/// (the draw/restore). Both are constructed from the same pool; the per-operation transaction is
/// begun off `self.pool` and the company is bound onto it explicitly via `bind_company_on`.
pub struct TimeoffRequestWriteService {
    pool: PgPool,
    requests: TimeoffRequestRepository,
    balances: TimeoffBalanceRepository,
}

impl TimeoffRequestWriteService {
    pub fn new(pool: PgPool) -> Self {
        let requests = TimeoffRequestRepository::new(pool.clone());
        let balances = TimeoffBalanceRepository::new(pool.clone());
        Self { pool, requests, balances }
    }

    /// Approve a timeoff request — THE invariant. Draws down the employee's balance for the timeoff
    /// type/period, GATED so `used` never exceeds `allocated`, in the SAME transaction as the
    /// `pending → approved` transition. If the balance is insufficient (or missing), both are rolled
    /// back and nothing changes.
    ///
    /// Ported from backbone-hr's `approve_leave`. `now` is dropped here (the timeoff schema has no
    /// `approved_at` column; the audit `updated_at` is stamped by the table trigger) — kept on the
    /// signature only where it would be bound.
    pub async fn approve_request(
        &self,
        timeoff_request_id: Uuid,
        approver: Option<Uuid>,
    ) -> Result<(), TimeoffError> {
        // RLS scope (ADR-0008), ID-only pattern: identified by the request id alone. The read rides
        // the request-dedicated connection; the company read off the row then binds the transaction
        // below, so the transition + the balance draw are both fenced even for non-request callers.
        let app = self.requests.find_for_approval(&self.pool, timeoff_request_id).await?
            .ok_or(TimeoffError::NotFound("timeoff request"))?;
        if app.status != "pending" {
            return Err(TimeoffError::InvalidState("timeoff request is not pending"));
        }
        let company_id = app.company_id;
        let employee_id = app.employee_id;
        let timeoff_type_id = app.timeoff_type_id;
        let days = app.days;
        // `is_paid` is read for the (future) `RequestApproved` event payload — unused until the
        // outbox/compound-event phase lands (ADR-005). Bound here to keep the read faithful to
        // backbone-hr's `find_for_approval`.
        let _is_paid = app.is_paid;
        // The `period` for the balance lookup = the `date_start` year. backbone-hr keys a balance on
        // `year` (INTEGER); timeoff keys it on `period` (TEXT), so the year is stringified.
        let period = app.date_start.year().to_string();

        let mut tx = self.pool.begin().await?;
        company_scope::bind_company_on(&mut tx, company_id).await?;
        // Claim the transition first (write-once), then draw the balance under the same tx.
        let moved = self.requests
            .mark_approved(&mut tx, timeoff_request_id, approver)
            .await?;
        if moved != 1 {
            tx.rollback().await?;
            return Err(TimeoffError::InvalidState("timeoff request is not pending"));
        }
        // Gate on availability: draw only if used + days <= allocated.
        let drawn = self.balances
            .draw(&mut tx, employee_id, timeoff_type_id, &period, days)
            .await?;
        if drawn != 1 {
            tx.rollback().await?;
            return Err(TimeoffError::InsufficientBalance);
        }
        tx.commit().await?;
        // TODO(events): emit `RequestApproved { timeoff_request_id, employee_id, company_id,
        // timeoff_type_id, days, is_paid }` via the outbox/compound-event sink once that phase lands
        // (ADR-005). backbone-hr emits `LeaveApproved` through `HrEventSink::publish` here; the
        // no-op deferral is safe because the balance draw already committed under the DB CHECK
        // backstop — a missed event never corrupts the balance.
        Ok(())
    }

    /// Reject a pending timeoff request (no balance change). Ported from backbone-hr's `reject_leave`.
    pub async fn reject_request(&self, timeoff_request_id: Uuid) -> Result<(), TimeoffError> {
        // RLS scope (ADR-0008), ID-only pattern: no company argument — the write rides the
        // request-dedicated connection, so another company's request is simply not matched.
        let moved = self.requests.mark_rejected(&self.pool, timeoff_request_id).await?;
        if moved != 1 {
            return Err(TimeoffError::InvalidState("timeoff request is not pending"));
        }
        // TODO(events): emit `RequestRejected` via the outbox/compound-event sink (ADR-005).
        Ok(())
    }

    /// Cancel a timeoff request. If it was APPROVED, restores the drawn-down balance in the same tx
    /// as the transition (so a balance is never left short); a pending one just cancels.
    ///
    /// Ported from backbone-hr's `cancel_leave`.
    pub async fn cancel_request(&self, timeoff_request_id: Uuid) -> Result<(), TimeoffError> {
        // RLS scope (ADR-0008), ID-only pattern: identified by the request id alone. The read rides
        // the request-dedicated connection and now also carries `company_id`, so the restore
        // transaction below can be bound explicitly (correct for non-request callers too).
        let app = self.requests.find_for_cancel(&self.pool, timeoff_request_id).await?
            .ok_or(TimeoffError::NotFound("timeoff request"))?;
        let company_id = app.company_id;
        let status = app.status.as_str();
        if status == "pending" {
            let m = self.requests.cancel_pending(&self.pool, timeoff_request_id).await?;
            return if m == 1 { Ok(()) } else { Err(TimeoffError::InvalidState("not cancellable")) };
        }
        if status != "approved" {
            return Err(TimeoffError::InvalidState("only a pending or approved request can be cancelled"));
        }
        let employee_id = app.employee_id;
        let timeoff_type_id = app.timeoff_type_id;
        let days = app.days;
        let period = app.date_start.year().to_string();

        let mut tx = self.pool.begin().await?;
        company_scope::bind_company_on(&mut tx, company_id).await?;
        let moved = self.requests.cancel_approved(&mut tx, timeoff_request_id).await?;
        if moved != 1 {
            tx.rollback().await?;
            return Err(TimeoffError::InvalidState("timeoff request is not approved"));
        }
        // Restore is GATED on `used >= days` so a tampered request (its `days` mutated after approval
        // via the generic PATCH surface) can never drive `used` negative and manufacture phantom
        // entitlement. The DB CHECK `used >= 0` is the backstop for any other writer; this gate turns
        // the violation into a clean domain error instead of a raw error.
        let restored = self.balances
            .restore(&mut tx, employee_id, timeoff_type_id, &period, days)
            .await?;
        if restored != 1 {
            tx.rollback().await?;
            return Err(TimeoffError::InvalidState(
                "cannot restore timeoff balance — the request's days exceed what was drawn",
            ));
        }
        tx.commit().await?;
        // TODO(events): emit `RequestCancelled` (with `was_approved`) via the outbox/compound-event
        // sink once that phase lands (ADR-005).
        Ok(())
    }
}
