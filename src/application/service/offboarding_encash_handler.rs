//! Consumer for the `offboarding.closed` compound event — leave encashment side (ADR-005).
//!
//! The timeoff module owns the APPLY side of the leave encashment at offboarding: on each
//! `offboarding.closed` envelope it zeroes the leaver's remaining leave balance (`used → allocated`),
//! **idempotently**. Registered on the integration bus in backbone-hr-app's `main.rs` alongside the
//! employee `OffboardingClosedHandler` and the payroll `OffboardingSettlementHandler` (the bus fans one
//! event out to all three; each target dedups via its own inbox consumer name).
//!
//! ## Division of labour at close
//!
//! The real 🇮🇩 unused-leave payout AMOUNT is computed inside the pesangon breakdown
//! (`unused_leave_payout`, carried in the `offboarding.closed` payload) and written as money by
//! payroll's `OffboardingSettlementHandler`. This handler does NOT recompute money — it only zeroes the
//! leave balance so it is not left dangling after the employee leaves. `used = allocated` makes the
//! remaining days show as consumed (paid out), which is the correct end state for a departed employee.
//!
//! ## Idempotency
//!
//! The relay is at-least-once, so this handler MUST be idempotent — and the UPDATE is naturally
//! idempotent (re-applying `used = allocated` once `used = allocated` is a no-op). It additionally
//! wraps the UPDATE in [`backbone_outbox::inbox::once`]: the `(consumer, event_id)` claim and the UPDATE
//! run in ONE transaction and commit together, so a redelivery is a pure no-op (the inbox returns
//! `false` and the UPDATE is skipped). The `used < allocated` guard means a fully-consumed balance
//! (already zeroed) is untouched even without the inbox.
//!
//! This is a user-owned custom file — it is NEVER regenerated.

use async_trait::async_trait;
use backbone_messaging::{EventError, IntegrationEventEnvelope, IntegrationEventHandler};
use backbone_outbox::inbox;
use sqlx::PgPool;
use uuid::Uuid;

/// The consumer name stamped into the timeoff inbox. Scoped so this leave-encashment target is
/// distinct from the other two `offboarding.closed` consumers (`offboarding.role` in employee,
/// `offboarding.settlement` in payroll). The ADR-005 idempotency key for this target is
/// `("offboarding.encash", event_id)`; the `event_id` arrives as the envelope id (preserved from the
/// outbox row id through the relay).
const CONSUMER: &str = "offboarding.encash";

/// Integration-event handler that zeroes the leaver's remaining leave balance on `offboarding.closed`,
/// idempotently. Holds only the pool — the apply is one UPDATE inside an `inbox`-guarded transaction.
pub struct OffboardingEncashHandler {
    pool: PgPool,
}

impl OffboardingEncashHandler {
    /// Create a new handler bound to the given pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IntegrationEventHandler for OffboardingEncashHandler {
    async fn handle(&self, envelope: IntegrationEventEnvelope) -> Result<(), EventError> {
        // The envelope id IS the outbox row's id (the relay preserves it) → the dedup key.
        let event_id = Uuid::parse_str(&envelope.id)
            .map_err(|e| handler_err(format!("bad envelope id '{}': {e}", envelope.id)))?;

        let p = &envelope.payload;
        let employee_id: Uuid = json_field(p, "employee_id")?;

        let mut tx = self.pool.begin().await.map_err(map_db)?;

        // Claim the event in-tx with the effect: the inbox row + the balance-zeroing UPDATE commit
        // together (or roll back together).
        let first_time = inbox::once(&mut *tx, "timeoff", CONSUMER, event_id)
            .await
            .map_err(|e| handler_err(format!("inbox claim: {e}")))?;

        if first_time {
            // Zero the remaining leave: every non-deleted balance row with unused days becomes
            // fully consumed (`used = allocated`). The `used < allocated` guard skips rows that are
            // already paid out / fully used — and makes the UPDATE idempotent even without the inbox.
            sqlx::query(
                r#"UPDATE timeoff.timeoff_balances
                      SET used = allocated
                    WHERE employee_id = $1
                      AND used < allocated"#,
            )
            .bind(employee_id)
            .execute(&mut *tx)
            .await
            .map_err(map_db)?;
        }

        tx.commit().await.map_err(map_db)?;
        Ok(())
    }

    fn event_patterns(&self) -> Vec<&'static str> {
        // Same pattern as the other two offboarding.closed consumers — the bus dispatches one event to
        // all three; each dedups via its own consumer name.
        vec!["offboarding.closed"]
    }

    fn name(&self) -> &'static str {
        "OffboardingEncashHandler"
    }
}

/// Decode a required payload field, mapping any failure to a handler error (so the bus reports a
/// precise malformed-payload message rather than a generic serde blob).
fn json_field<T>(p: &serde_json::Value, field: &str) -> Result<T, EventError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(p[field].clone())
        .map_err(|e| handler_err(format!("payload.{field}: {e}")))
}

fn map_db(e: sqlx::Error) -> EventError {
    handler_err(format!("db: {e}"))
}

fn handler_err(message: String) -> EventError {
    EventError::handler(CONSUMER, message)
}
