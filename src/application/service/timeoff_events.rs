//! Timeoff domain events (hand-authored, user-owned) — the public extension surface.
//!
//! backbone-timeoff posts NO GL and owns no cross-module tables. Its one outbound seam is the
//! leave lifecycle's settlement: a request reached an outcome downstream projections may act on
//! (`LeaveSettled`). The two named consumers are the timesheet analytic row's leave regeneration
//! (the host adapter expands the window into per-day hours via backbone-calendar working time and
//! calls the timesheet verb) and payroll leave settlement. A consuming service supplies the sink
//! (bus, outbox, …); the default is a logging no-op (ADR-0004: no crate edge).

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How a leave request settled.
///
/// `Voided` is the zero-days grant: the request was approved but its window settles to no
/// absence (an empty/inverted span), so downstream row-generation has nothing to insert — the
/// settlement is still published so consumers can clear any rows a previous generation left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaveSettlement {
    Approved,
    Refused,
    Cancelled,
    Voided,
}

/// A leave request settled into a final outcome. The window travels as-is; per-day expansion
/// (hours per working day, holiday carving) is the HOST adapter's job, not the event's.
///
/// `company_id` is the legacy tenancy twin (ADR-0029): timeoff itself is tenant-agnostic, but the
/// leave consumers (the timesheet row regeneration, payroll settlement) still key on one. The write
/// service sources it from the ambient org scope's legacy company id and fails closed when no scope
/// is bound — it never guesses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LeaveSettled {
    /// Legacy company twin for the leave consumers; sourced from the ambient org scope,
    /// fail-closed (ADR-0029).
    pub company_id: Uuid,
    pub request_id: Uuid,
    pub employee_id: Uuid,
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    pub settlement: LeaveSettlement,
}

/// The timeoff domain-event union.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum TimeoffEvent {
    LeaveSettled(LeaveSettled),
}

/// Sink the write path publishes to (after the settling transaction commits — a rolled-back
/// verb never emits). A consuming service supplies its own (bus, outbox, …).
pub trait TimeoffEventSink: Send + Sync {
    fn publish(&self, event: &TimeoffEvent);
}

/// A no-op/logging sink for tests and single-process composition.
#[derive(Debug, Default, Clone)]
pub struct LoggingSink;

impl TimeoffEventSink for LoggingSink {
    fn publish(&self, event: &TimeoffEvent) {
        tracing::info!(?event, "timeoff event");
    }
}
