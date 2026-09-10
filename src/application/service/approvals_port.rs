//! The approvals seam (Wave 1 P1, H-2) — the port trait the composing app
//! implements against backbone-approvals once the H-9 decision engine lands.
//!
//! ADR-0004: shipped libraries keep ZERO normal Cargo edges on each other, so
//! timeoff cannot depend on the approvals crate. The link is data + behavior:
//! `timeoff_requests.approval_request_id` (a logical FK, no DB constraint
//! across module schemas) + this port, supplied at composition time.
//!
//! P1 scope is the SEAM ONLY (locked decision, 2026-08-16): the verbs create
//! and honor the link; the decision engine itself lands with H-9. Until the
//! app wires a real port, [`UnwiredApprovals`] is the default and the module
//! behaves exactly as before — requests are approved directly by the manager
//! verbs, and no request carries `approval_request_id` unless someone set it
//! out-of-band (which `approve_request` then fails CLOSED on, see TR2).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The verdict on a filed approval, as read back through the port. Deliberately
/// a mirror of approvals' status vocabulary restricted to what the timeoff
/// verbs need — the engine's richer states (escalated, delegated, …) all read
/// as "not yet approved" from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalVerdict {
    /// Awaiting a decision.
    Pending,
    /// Granted.
    Approved,
    /// Refused (sticky — the engine does not re-ask).
    Rejected,
    /// Withdrawn by the requester.
    Cancelled,
}

/// What timeoff files for approval: WHO wants WHAT for HOW LONG, plus the
/// back-reference so the engine's notifications link back to the request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalFilingRequest {
    /// The company scope (stamped onto the ApprovalRequest for its own fence).
    ///
    /// Legacy tenancy twin (ADR-0029): timeoff itself is tenant-agnostic, but the receiving
    /// approvals books still key on one. The write service sources it from the ambient org
    /// scope's legacy company id and fails closed when no scope is bound — it never guesses.
    pub company_id: Uuid,
    /// The timeoff request the filing is about (correlation id).
    pub timeoff_request_id: Uuid,
    /// The applicant # logical FK to employee.Employee.id.
    pub employee_id: Uuid,
    /// The timeoff type requested # logical FK to TimeoffType.id.
    pub timeoff_type_id: Uuid,
    /// First day of leave (inclusive).
    pub date_start: chrono::NaiveDate,
    /// Last day of leave (inclusive).
    pub date_end: chrono::NaiveDate,
    /// The number of days asked for (the engine shows it to the approver).
    pub days: rust_decimal::Decimal,
    /// Applicant note, if any.
    pub note: Option<String>,
    /// When the request was submitted.
    pub submitted_at: DateTime<Utc>,
}

/// Errors from the approvals seam. `Unwired` is the load-bearing variant: it is
/// what the default [`UnwiredApprovals`] returns, and what `approve_request`
/// converts into a fail-closed `ApprovalSeam` error when a request carries an
/// `approval_request_id` but no port is wired.
#[derive(Debug, thiserror::Error)]
pub enum ApprovalSeamError {
    #[error("the approvals seam is not wired — supply an ApprovalFiling port to use linked approvals")]
    Unwired,
    #[error("approval request {0} not found on the approvals side")]
    UnknownApprovalRequest(Uuid),
    #[error("approvals port transport error: {0}")]
    Transport(String),
}

/// The port (ADR-0004 serialized-port pattern). Implemented by the composing
/// app against backbone-approvals; `timeoff` only ever speaks this trait.
#[async_trait::async_trait]
pub trait ApprovalFiling: Send + Sync {
    /// File a new approval request for a submitted timeoff request; returns
    /// the created `approvals.ApprovalRequest.id` to stamp onto
    /// `timeoff_requests.approval_request_id`.
    async fn file(&self, req: &ApprovalFilingRequest) -> Result<Uuid, ApprovalSeamError>;

    /// Read back the verdict for a previously filed approval.
    async fn status(&self, approval_request_id: Uuid) -> Result<ApprovalVerdict, ApprovalSeamError>;
}

/// The default port: nothing is wired. Filing fails loudly (a caller asking for
/// tracked approvals without wiring the engine gets an explicit error, not a
/// silently untracked request); status lookups fail closed for the same reason.
pub struct UnwiredApprovals;

#[async_trait::async_trait]
impl ApprovalFiling for UnwiredApprovals {
    async fn file(&self, _req: &ApprovalFilingRequest) -> Result<Uuid, ApprovalSeamError> {
        Err(ApprovalSeamError::Unwired)
    }

    async fn status(&self, approval_request_id: Uuid) -> Result<ApprovalVerdict, ApprovalSeamError> {
        Err(ApprovalSeamError::UnknownApprovalRequest(approval_request_id))
    }
}
