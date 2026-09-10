//! `TimeoffQueryService` impl for [`crate::TimeoffModule`].
//!
//! Hand-written (user-owned — see `metaphor.codegen.yaml`). The generated `exports/services.rs`
//! declares the `TimeoffQueryService` port trait but no impl is generated for it; this file is
//! that impl. It is the seam every other module consumes timeoff through.
//!
//! Split:
//! - the **standard lookups** (`get_*` / `*_exists`) delegate to the existing `GenericCrudService`
//!   (already wired on the module) and map entity → public DTO.
//! - the **custom read-port** `paid_leave_days` delegates to
//!   [`TimeoffRequestRepository::paid_leave_days`], which holds the hand-written SQL (4-layer rule:
//!   services orchestrate, repos hold SQL).
//!
//! Tenancy (ADR-0029): no scoping is done here — the module is tenant-agnostic. When the
//! composing service binds an ambient org scope, the repo's reads ride the request-dedicated
//! connection it holds and the decorator-installed row-level fence owns isolation; `find_by_id`
//! and `paid_leave_days` both honor it.

use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDate;
use uuid::Uuid;

use crate::domain::entity::{TimeoffBalance, TimeoffRequest, TimeoffType};
// `exports::services` and `exports::types` are both private modules; their items are re-exported at
// `crate::exports::` — import through that, not the private module paths.
use crate::exports::TimeoffQueryService;
use crate::exports::{
    TimeoffBalanceDto, TimeoffBalanceId, TimeoffBalanceSummary, TimeoffRequestDto, TimeoffRequestId,
    TimeoffRequestSummary, TimeoffTypeDto, TimeoffTypeId, TimeoffTypeSummary,
};
// The `*Id` names here are the EXPORT (public) newtypes, deliberately — the domain entity also
// defines same-named id newtypes, so import only the three entity STRUCTS above (not
// `domain::entity::*`) to avoid a name collision.
use crate::TimeoffModule;

#[async_trait]
impl TimeoffQueryService for TimeoffModule {
    async fn get_timeoff_balance(&self, id: TimeoffBalanceId) -> Result<Option<TimeoffBalanceDto>> {
        let entity = self
            .timeoff_balance_service
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(entity.map(timeoff_balance_to_dto).transpose()?)
    }

    async fn get_timeoff_balance_summary(
        &self,
        id: TimeoffBalanceId,
    ) -> Result<Option<TimeoffBalanceSummary>> {
        let entity = self
            .timeoff_balance_service
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(entity.map(|e| TimeoffBalanceSummary { id: TimeoffBalanceId(e.id) }))
    }

    async fn timeoff_balance_exists(&self, id: TimeoffBalanceId) -> Result<bool> {
        Ok(self
            .timeoff_balance_service
            .find_by_id(&id.into_inner().to_string())
            .await?
            .is_some())
    }

    async fn get_timeoff_request(&self, id: TimeoffRequestId) -> Result<Option<TimeoffRequestDto>> {
        let entity = self
            .timeoff_request_service
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(entity.map(timeoff_request_to_dto).transpose()?)
    }

    async fn get_timeoff_request_summary(
        &self,
        id: TimeoffRequestId,
    ) -> Result<Option<TimeoffRequestSummary>> {
        let entity = self
            .timeoff_request_service
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(entity.map(|e| TimeoffRequestSummary { id: TimeoffRequestId(e.id), status: e.status }))
    }

    async fn timeoff_request_exists(&self, id: TimeoffRequestId) -> Result<bool> {
        Ok(self
            .timeoff_request_service
            .find_by_id(&id.into_inner().to_string())
            .await?
            .is_some())
    }

    async fn get_timeoff_type(&self, id: TimeoffTypeId) -> Result<Option<TimeoffTypeDto>> {
        let entity = self
            .timeoff_type_service
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(entity.map(timeoff_type_to_dto).transpose()?)
    }

    async fn get_timeoff_type_summary(
        &self,
        id: TimeoffTypeId,
    ) -> Result<Option<TimeoffTypeSummary>> {
        let entity = self
            .timeoff_type_service
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(entity.map(|e| TimeoffTypeSummary { id: TimeoffTypeId(e.id), name: e.name }))
    }

    async fn timeoff_type_exists(&self, id: TimeoffTypeId) -> Result<bool> {
        Ok(self
            .timeoff_type_service
            .find_by_id(&id.into_inner().to_string())
            .await?
            .is_some())
    }

    async fn paid_leave_days(
        &self,
        employee_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<NaiveDate>> {
        Ok(self
            .timeoff_request_repository
            .paid_leave_days(&self.db_pool, employee_id, from, to)
            .await?)
    }
}

// ─── entity → public DTO mapping ───────────────────────────────────────────────
//
// The only non-trivial conversion is `metadata`: the entity holds a typed `AuditMetadata`, the
// public DTO exposes it as an opaque `serde_json::Value` (so consumers don't depend on the internal
// audit struct's shape).

fn timeoff_balance_to_dto(e: TimeoffBalance) -> Result<TimeoffBalanceDto> {
    Ok(TimeoffBalanceDto {
        id: TimeoffBalanceId(e.id),
        timeoff_type_id: e.timeoff_type_id,
        employee_id: e.employee_id,
        period: e.period,
        allocated: e.allocated,
        used: e.used,
        // Accrual walk state (Wave 1 P1, H-2): plan link, validity window,
        // watermark, postponed carry, expiry stamp.
        accrual_plan_id: e.accrual_plan_id,
        date_from: e.date_from,
        date_to: e.date_to,
        last_accrual_at: e.last_accrual_at,
        carried_over: e.carried_over,
        expired_at: e.expired_at,
        metadata: serde_json::to_value(&e.metadata)?,
    })
}

fn timeoff_request_to_dto(e: TimeoffRequest) -> Result<TimeoffRequestDto> {
    Ok(TimeoffRequestDto {
        id: TimeoffRequestId(e.id),
        timeoff_type_id: e.timeoff_type_id,
        employee_id: e.employee_id,
        date_start: e.date_start,
        date_end: e.date_end,
        note: e.note,
        approval_employee_id: e.approval_employee_id,
        // The approvals seam link (Wave 1 P1, H-2).
        approval_request_id: e.approval_request_id,
        note_reject: e.note_reject,
        status: e.status,
        metadata: serde_json::to_value(&e.metadata)?,
    })
}

fn timeoff_type_to_dto(e: TimeoffType) -> Result<TimeoffTypeDto> {
    Ok(TimeoffTypeDto {
        id: TimeoffTypeId(e.id),
        name: e.name,
        code: e.code,
        is_paid: e.is_paid,
        allow_carry_forward: e.allow_carry_forward,
        metadata: serde_json::to_value(&e.metadata)?,
    })
}
