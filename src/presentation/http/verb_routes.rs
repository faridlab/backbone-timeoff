//! The leave lifecycle over HTTP: submit, approve, reject, cancel.
//!
//! The verbs live in the write service (the approvals filing, the
//! tx-gated balance drawdown, the settlement events); this file only faces
//! them. Like the timesheet guarded surface, every handler extracts the
//! org context the composing service's guard inserted and derives nothing
//! tenant-shaped itself — the write path relays the ambient request scope,
//! so the composing decorator's fence decides what each verb may touch.
//!
//! The bodies are deliberately narrow: submit names the employee, the type,
//! the window and a note; the settling verbs take the request id alone (the
//! approver stamp is optional on approve — the engine-side record is the
//! authority). Nothing here exposes generic writes.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use backbone_auth::org::OrgContext;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::application::service::timeoff_request_service_custom::{
    TimeoffError, TimeoffRequestWriteService,
};

fn err_response(e: TimeoffError) -> Response {
    use TimeoffError::*;
    let (status, code) = match &e {
        NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
        InvalidState(m) => (StatusCode::CONFLICT, "invalid_state"),
        InsufficientBalance => (StatusCode::CONFLICT, "insufficient_balance"),
        ApprovalNotGranted => (StatusCode::CONFLICT, "approval_not_granted"),
        NoCompanyScope => (StatusCode::INTERNAL_SERVER_ERROR, "no_org_scope"),
        ApprovalSeam(_) => (StatusCode::INTERNAL_SERVER_ERROR, "approvals_seam_error"),
        Db(_) => (StatusCode::INTERNAL_SERVER_ERROR, "database_error"),
    };
    (status, Json(json!({ "error": code, "message": e.to_string() }))).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitBody {
    employee_id: Uuid,
    timeoff_type_id: Uuid,
    date_start: chrono::NaiveDate,
    date_end: chrono::NaiveDate,
    #[serde(default)]
    note: Option<String>,
    /// Which part of the day: "full" (default) | "am" | "pm" — am/pm only on
    /// a single-day ask.
    #[serde(default)]
    part: Option<String>,
    /// The certificate/sick note on file (bucket file ref).
    #[serde(default)]
    attachment_file_id: Option<Uuid>,
    #[serde(default)]
    attachment_note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveBody {
    #[serde(default)]
    approver_id: Option<Uuid>,
}

/// The guarded leave verbs. Mount under the host's authenticated
/// (org-guarded, tenant-routed) tree; reads ride the module's own generic
/// read surface.
pub fn create_timeoff_verb_routes(svc: Arc<TimeoffRequestWriteService>) -> Router {
    Router::new()
        .route("/requests/submit", post(submit))
        .route("/requests/:request_id/approve", post(approve))
        .route("/requests/:request_id/reject", post(reject))
        .route("/requests/:request_id/cancel", post(cancel))
        .with_state(svc)
}

async fn submit(
    State(svc): State<Arc<TimeoffRequestWriteService>>,
    _org: OrgContext,
    Json(b): Json<SubmitBody>,
) -> Response {
    let part: &'static str = match b.part.as_deref() {
        None | Some("full") => "full",
        Some("am") => "am",
        Some("pm") => "pm",
        Some(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "error": "bad_part",
                    "message": "part must be one of: full, am, pm"
                })),
            )
                .into_response()
        }
    };
    match svc
        .submit_request_part(
            b.timeoff_type_id,
            b.employee_id,
            b.date_start,
            b.date_end,
            b.note,
            part,
            b.attachment_file_id,
            b.attachment_note,
        )
        .await
    {
        Ok(id) => (StatusCode::CREATED, Json(json!({ "id": id }))).into_response(),
        Err(e) => err_response(e),
    }
}

async fn approve(
    State(svc): State<Arc<TimeoffRequestWriteService>>,
    _org: OrgContext,
    Path(request_id): Path<Uuid>,
    body: Option<Json<ApproveBody>>,
) -> Response {
    let approver = body.and_then(|Json(b)| b.approver_id);
    match svc.approve_request(request_id, approver).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err_response(e),
    }
}

async fn reject(
    State(svc): State<Arc<TimeoffRequestWriteService>>,
    _org: OrgContext,
    Path(request_id): Path<Uuid>,
) -> Response {
    match svc.reject_request(request_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err_response(e),
    }
}

async fn cancel(
    State(svc): State<Arc<TimeoffRequestWriteService>>,
    _org: OrgContext,
    Path(request_id): Path<Uuid>,
) -> Response {
    match svc.cancel_request(request_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err_response(e),
    }
}
