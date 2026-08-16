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
use chrono::{Datelike, Utc};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::infrastructure::persistence::{
    TimeoffBalanceRepository, TimeoffRequestRepository, TimeoffRequestDraft,
};

use super::approvals_port::{ApprovalFiling, ApprovalFilingRequest, UnwiredApprovals};

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
    /// TR2 (Wave 1 P1): the request is linked to an approvals.ApprovalRequest
    /// whose verdict is not `approved` — the timeoff verb refuses to grant.
    #[error("approval not granted for the linked approval request")]
    ApprovalNotGranted,
    /// The approvals seam itself failed (unwired port, unknown filing,
    /// transport). Failing closed: a request linked into the engine never
    /// bypasses it via a seam error.
    #[error("approvals seam: {0}")]
    ApprovalSeam(#[from] super::approvals_port::ApprovalSeamError),
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
    /// The approvals seam (Wave 1 P1, H-2). Defaults to [`UnwiredApprovals`] —
    /// the module behaves exactly as before until the composing app wires a
    /// real port against backbone-approvals (H-9). ADR-0004: no crate edge.
    approvals: Arc<dyn ApprovalFiling>,
}

impl TimeoffRequestWriteService {
    pub fn new(pool: PgPool) -> Self {
        let requests = TimeoffRequestRepository::new(pool.clone());
        let balances = TimeoffBalanceRepository::new(pool.clone());
        Self { pool, requests, balances, approvals: Arc::new(UnwiredApprovals) }
    }

    /// Supply the approvals port (the composing app's adapter against
    /// backbone-approvals). After this, `submit_request` files every request
    /// and `approve_request` honors the engine's verdict (TR2).
    pub fn with_approvals(mut self, port: Arc<dyn ApprovalFiling>) -> Self {
        self.approvals = port;
        self
    }

    /// Submit a new leave request (Wave 1 P1): creates it `pending`, and — when
    /// the approvals seam is wired — files it with the engine and stamps the
    /// link. File-first ordering: the filing carries the client-generated
    /// request id, so the insert lands with `approval_request_id` already set;
    /// a wiring failure fails the submit (no silently untracked request).
    pub async fn submit_request(
        &self,
        company_id: Uuid,
        timeoff_type_id: Uuid,
        employee_id: Uuid,
        date_start: chrono::NaiveDate,
        date_end: chrono::NaiveDate,
        note: Option<String>,
    ) -> Result<Uuid, TimeoffError> {
        let request_id = Uuid::new_v4();
        // File first (outside any tx — the port is a network call to the
        // approvals side; holding a row lock across it is worse than an orphaned
        // filing on a failed insert, which the H-9 engine's sweeper can reap).
        let filing = ApprovalFilingRequest {
            company_id,
            timeoff_request_id: request_id,
            employee_id,
            timeoff_type_id,
            date_start,
            date_end,
            days: chrono_days_inclusive(date_start, date_end),
            note: note.clone(),
            submitted_at: Utc::now(),
        };
        let approval_request_id = match self.approvals.file(&filing).await {
            Ok(id) => Some(id),
            // Unwired seam = this deployment doesn't track approvals: the
            // request simply carries no link (module behaves as pre-P1).
            Err(super::approvals_port::ApprovalSeamError::Unwired) => None,
            // A WIRED port that fails is a real failure — fail the submit
            // rather than create a request the engine doesn't know about.
            Err(e) => return Err(e.into()),
        };

        let mut tx = self.pool.begin().await?;
        company_scope::bind_company_on(&mut tx, company_id).await?;
        let draft = TimeoffRequestDraft {
            id: request_id,
            company_id,
            timeoff_type_id,
            employee_id,
            date_start,
            date_end,
            note,
            approval_request_id,
        };
        let inserted = self.requests.insert_pending(&mut tx, &draft).await?;
        if inserted != 1 {
            tx.rollback().await?;
            return Err(TimeoffError::InvalidState("request rejected — check the timeoff type"));
        }
        tx.commit().await?;
        Ok(request_id)
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
        let date_start = app.date_start;
        let date_end = app.date_end;

        // TR2 (Wave 1 P1, H-2): a request linked into the approvals engine is
        // granted only by the engine. The seam check happens BEFORE the tx: the
        // verdict read is a port call (network), and the transition below only
        // moves pending rows anyway, so a verdict flip mid-tx can at worst turn
        // a would-be approval into this same error on retry.
        if let Some(approval_request_id) = app.approval_request_id {
            use super::approvals_port::{ApprovalSeamError, ApprovalVerdict};
            match self.approvals.status(approval_request_id).await {
                Ok(ApprovalVerdict::Approved) => {}
                Ok(_) => return Err(TimeoffError::ApprovalNotGranted),
                // Unwired port + a linked request = out-of-band linkage (PATCH)
                // or a deployment regression — fail CLOSED: never bypass the
                // engine a request was filed into.
                Err(ApprovalSeamError::Unwired) | Err(ApprovalSeamError::UnknownApprovalRequest(_)) => {
                    return Err(TimeoffError::ApprovalNotGranted);
                }
                Err(e) => return Err(e.into()),
            }
        }
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
        // Gate on availability AND the accrual validity window: draw only if
        // `used + days <= allocated` and `[date_start, date_end]` fits the
        // balance's `[date_from, date_to]` (open bounds when NULL).
        let drawn = self.balances
            .draw(&mut tx, employee_id, timeoff_type_id, &period, days, date_start, date_end)
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

/// The inclusive day span `[start, end]` as Decimal — the same span SQL the
/// approve path computes (`date_end - date_start + 1`), kept identical here so
/// the filed approval shows the days the draw will take.
fn chrono_days_inclusive(start: chrono::NaiveDate, end: chrono::NaiveDate) -> rust_decimal::Decimal {
    use rust_decimal::prelude::ToPrimitive;
    let days = (end - start).num_days() + 1;
    rust_decimal::Decimal::from(days.max(0).to_i64().unwrap_or(0))
}
