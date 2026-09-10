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
//! logic is identical. Event emission follows backbone-hr's `HrEventSink` shape through the
//! module's own `TimeoffEventSink`: each settling verb publishes `LeaveSettled` AFTER its
//! transaction commits (a rolled-back verb never emits).
//!
//! Tenancy (ADR-0029): the module carries no tenancy of its own. Its own transactions relay the
//! AMBIENT org scope the COMPOSING service bound (`org_scope::bind_org_scope_on`), so the
//! decorator-installed org-unit fill and row-level fence apply; undecorated deployments run the
//! transaction plain. The outbound seams that still key on a company (the approvals filing, the
//! settlement events) carry the scope's legacy company twin — fail-closed when no scope is bound.

use backbone_orm::org_scope;
use chrono::{Datelike, Utc};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::infrastructure::persistence::{
    TimeoffBalanceRepository, TimeoffRequestRepository, TimeoffRequestDraft,
};

use super::approvals_port::{ApprovalFiling, ApprovalFilingRequest, UnwiredApprovals};
use super::timeoff_events::{LeaveSettlement, LeaveSettled, LoggingSink, TimeoffEvent, TimeoffEventSink};

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
    #[error("no org scope bound: the composing service must resolve one for this request")]
    NoCompanyScope,
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
/// begun off `self.pool` and the ambient org scope is relayed onto it (when one is bound).
pub struct TimeoffRequestWriteService {
    pool: PgPool,
    requests: TimeoffRequestRepository,
    balances: TimeoffBalanceRepository,
    /// The approvals seam (Wave 1 P1, H-2). Defaults to [`UnwiredApprovals`] —
    /// the module behaves exactly as before until the composing app wires a
    /// real port against backbone-approvals (H-9). ADR-0004: no crate edge.
    approvals: Arc<dyn ApprovalFiling>,
    /// The leave-lifecycle event seam. Defaults to [`LoggingSink`] — the module
    /// behaves exactly as before until the composing app wires a real sink
    /// (bus, outbox). ADR-0004: no crate edge.
    events: Arc<dyn TimeoffEventSink>,
}

impl TimeoffRequestWriteService {
    pub fn new(pool: PgPool) -> Self {
        let requests = TimeoffRequestRepository::new(pool.clone());
        let balances = TimeoffBalanceRepository::new(pool.clone());
        Self {
            pool,
            requests,
            balances,
            approvals: Arc::new(UnwiredApprovals),
            events: Arc::new(LoggingSink),
        }
    }

    /// The company id for the seams that still key on one — the approvals filing
    /// (`ApprovalFilingRequest`) and the settlement events. Sourced from the ambient org scope the
    /// COMPOSING service binds; absent → fail-closed. The module never guesses a company.
    fn legacy_company_id() -> Result<Uuid, TimeoffError> {
        org_scope::current_org_scope()
            .and_then(|s| s.legacy_company_id())
            .ok_or(TimeoffError::NoCompanyScope)
    }

    /// Supply the approvals port (the composing app's adapter against
    /// backbone-approvals). After this, `submit_request` files every request
    /// and `approve_request` honors the engine's verdict (TR2).
    pub fn with_approvals(mut self, port: Arc<dyn ApprovalFiling>) -> Self {
        self.approvals = port;
        self
    }

    /// Supply the leave-lifecycle event sink. After this, every settling verb
    /// (`approve_request` / `reject_request` / `cancel_request`) publishes
    /// `LeaveSettled` after its transaction commits.
    pub fn with_events(mut self, sink: Arc<dyn TimeoffEventSink>) -> Self {
        self.events = sink;
        self
    }

    /// Publish a settlement off the verb's committed state. Private: the only
    /// call sites sit immediately after a `tx.commit()` (or the lock-free
    /// pending transitions), so a rolled-back verb never emits.
    fn settle(
        &self,
        company_id: Uuid,
        request_id: Uuid,
        employee_id: Uuid,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
        settlement: LeaveSettlement,
    ) {
        self.events.publish(&TimeoffEvent::LeaveSettled(LeaveSettled {
            company_id,
            request_id,
            employee_id,
            date_from,
            date_to,
            settlement,
        }));
    }

    /// Submit a new leave request (Wave 1 P1): creates it `pending`, and — when
    /// the approvals seam is wired — files it with the engine and stamps the
    /// link. File-first ordering: the filing carries the client-generated
    /// request id, so the insert lands with `approval_request_id` already set;
    /// a wiring failure fails the submit (no silently untracked request).
    ///
    /// Tenancy (ADR-0029): the filing's `company_id` is the legacy twin the unstripped
    /// approvals books still key on — sourced from the ambient org scope, fail-closed.
    pub async fn submit_request(
        &self,
        timeoff_type_id: Uuid,
        employee_id: Uuid,
        date_start: chrono::NaiveDate,
        date_end: chrono::NaiveDate,
        note: Option<String>,
    ) -> Result<Uuid, TimeoffError> {
        // The legacy company key the approvals filing keys on (see the tenancy note above).
        let company_id = Self::legacy_company_id()?;
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
        // Relay the AMBIENT org scope (when the caller bound one) so the composing
        // decorator's org-unit fill and row-level fence apply to this insert.
        if let Some(scope) = org_scope::current_org_scope() {
            org_scope::bind_org_scope_on(&mut tx, &scope).await?;
        }
        let draft = TimeoffRequestDraft {
            id: request_id,
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
        // ID-only read (ADR-0029): identified by the request id alone. It rides the
        // request-dedicated connection when the composing service bound one, so a row its
        // tenancy decorator's fence excludes simply is not found.
        let app = self.requests.find_for_approval(&self.pool, timeoff_request_id).await?
            .ok_or(TimeoffError::NotFound("timeoff request"))?;
        if app.status != "pending" {
            return Err(TimeoffError::InvalidState("timeoff request is not pending"));
        }
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
        // Relay the AMBIENT org scope (when the caller bound one) so the composing
        // decorator's fence covers the transition + the balance draw.
        if let Some(scope) = org_scope::current_org_scope() {
            org_scope::bind_org_scope_on(&mut tx, &scope).await?;
        }
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
        // The grant settles: a zero-day window (empty/inverted span) is a VOID settlement:
        // the request is granted but carries no absence, so consumers generate no rows for it.
        let settlement =
            if days.is_zero() { LeaveSettlement::Voided } else { LeaveSettlement::Approved };
        // The event seam still keys on a company (the leave consumers): source the legacy twin
        // off the ambient org scope, fail-closed.
        let company_id = Self::legacy_company_id()?;
        self.settle(company_id, timeoff_request_id, employee_id, date_start, date_end, settlement);
        Ok(())
    }

    /// Reject a pending timeoff request (no balance change). Ported from backbone-hr's `reject_leave`.
    pub async fn reject_request(&self, timeoff_request_id: Uuid) -> Result<(), TimeoffError> {
        // ID-only (ADR-0029): no tenant argument — the gated UPDATE rides the request-dedicated
        // connection when one is bound, so the composing decorator's fence decides what is
        // rejectable; another tenant's request is simply not matched.
        let settled = self.requests.mark_rejected(&self.pool, timeoff_request_id).await?;
        let row = settled.ok_or(TimeoffError::InvalidState("timeoff request is not pending"))?;
        // The event seam still keys on a company (the leave consumers): source the legacy twin
        // off the ambient org scope, fail-closed.
        let company_id = Self::legacy_company_id()?;
        self.settle(
            company_id,
            timeoff_request_id,
            row.employee_id,
            row.date_start,
            row.date_end,
            LeaveSettlement::Refused,
        );
        Ok(())
    }

    /// Cancel a timeoff request. If it was APPROVED, restores the drawn-down balance in the same tx
    /// as the transition (so a balance is never left short); a pending one just cancels.
    ///
    /// Ported from backbone-hr's `cancel_leave`.
    pub async fn cancel_request(&self, timeoff_request_id: Uuid) -> Result<(), TimeoffError> {
        // ID-only read (ADR-0029): identified by the request id alone. It rides the
        // request-dedicated connection when the composing service bound one, so a row its
        // tenancy decorator's fence excludes simply is not found.
        let app = self.requests.find_for_cancel(&self.pool, timeoff_request_id).await?
            .ok_or(TimeoffError::NotFound("timeoff request"))?;
        let status = app.status.as_str();
        if status == "pending" {
            let m = self.requests.cancel_pending(&self.pool, timeoff_request_id).await?;
            if m != 1 {
                return Err(TimeoffError::InvalidState("not cancellable"));
            }
            // The event seam still keys on a company (the leave consumers): source the legacy
            // twin off the ambient org scope, fail-closed.
            let company_id = Self::legacy_company_id()?;
            self.settle(
                company_id,
                timeoff_request_id,
                app.employee_id,
                app.date_start,
                app.date_end,
                LeaveSettlement::Cancelled,
            );
            return Ok(());
        }
        if status != "approved" {
            return Err(TimeoffError::InvalidState("only a pending or approved request can be cancelled"));
        }
        let employee_id = app.employee_id;
        let timeoff_type_id = app.timeoff_type_id;
        let days = app.days;
        let period = app.date_start.year().to_string();

        let mut tx = self.pool.begin().await?;
        // Relay the AMBIENT org scope (when the caller bound one) so the composing
        // decorator's fence covers the transition + the balance restore.
        if let Some(scope) = org_scope::current_org_scope() {
            org_scope::bind_org_scope_on(&mut tx, &scope).await?;
        }
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
        // The event seam still keys on a company (the leave consumers): source the legacy twin
        // off the ambient org scope, fail-closed.
        let company_id = Self::legacy_company_id()?;
        self.settle(
            company_id,
            timeoff_request_id,
            employee_id,
            app.date_start,
            app.date_end,
            LeaveSettlement::Cancelled,
        );
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
