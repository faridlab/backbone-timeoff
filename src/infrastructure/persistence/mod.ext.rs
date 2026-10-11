// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// The accrual walk's SQL (Wave 1 P1, H-2): SKIP LOCKED claim, plan-ladder read,
// optimistic-guarded grant apply, expiry stamp. User-owned — see
// metaphor.codegen.yaml.
mod timeoff_accrual_repository;
pub use timeoff_accrual_repository::{
    AccrualLevelRow, AccrualWalkRow, TimeoffAccrualRepository,
};
// The submit verb's draft (client-generated id + optional approvals link).
pub use timeoff_request_repository::TimeoffRequestDraft;
