use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use rust_decimal::Decimal;

use super::AccrualStartType;
use super::AccrualFrequency;
use super::AccrualLostDaysAction;
use super::AuditMetadata;

/// Strongly-typed ID for TimeoffAccrualLevel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimeoffAccrualLevelId(pub Uuid);

impl TimeoffAccrualLevelId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for TimeoffAccrualLevelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for TimeoffAccrualLevelId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for TimeoffAccrualLevelId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<TimeoffAccrualLevelId> for Uuid {
    fn from(id: TimeoffAccrualLevelId) -> Self { id.0 }
}

impl AsRef<Uuid> for TimeoffAccrualLevelId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for TimeoffAccrualLevelId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TimeoffAccrualLevel {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub sequence: i32,
    pub start_count: Decimal,
    pub start_type: AccrualStartType,
    pub frequency: AccrualFrequency,
    pub added_value: Decimal,
    pub is_added_based_on_worked_time: bool,
    pub maximum_leave: Option<Decimal>,
    pub action_with_lost_days: AccrualLostDaysAction,
    pub postponed_max_days: Option<Decimal>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl TimeoffAccrualLevel {
    /// Create a builder for TimeoffAccrualLevel
    pub fn builder() -> TimeoffAccrualLevelBuilder {
        <TimeoffAccrualLevelBuilder as Default>::default()
    }

    /// Create a new TimeoffAccrualLevel with required fields
    pub fn new(plan_id: Uuid, sequence: i32, start_count: Decimal, start_type: AccrualStartType, frequency: AccrualFrequency, added_value: Decimal, is_added_based_on_worked_time: bool, action_with_lost_days: AccrualLostDaysAction) -> Self {
        Self {
            id: Uuid::new_v4(),
            plan_id,
            sequence,
            start_count,
            start_type,
            frequency,
            added_value,
            is_added_based_on_worked_time,
            maximum_leave: None,
            action_with_lost_days,
            postponed_max_days: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> TimeoffAccrualLevelId {
        TimeoffAccrualLevelId(self.id)
    }

    /// Get when this entity was created
    pub fn created_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.created_at.as_ref()
    }

    /// Get when this entity was last updated
    pub fn updated_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.updated_at.as_ref()
    }

    /// Check if this entity is soft deleted
    pub fn is_deleted(&self) -> bool {
        self.metadata.deleted_at.is_some()
    }

    /// Check if this entity is active (not deleted)
    pub fn is_active(&self) -> bool {
        self.metadata.deleted_at.is_none()
    }

    /// Get when this entity was deleted
    pub fn deleted_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.deleted_at.as_ref()
    }

    /// Get who created this entity
    pub fn created_by(&self) -> Option<&Uuid> {
        self.metadata.created_by.as_ref()
    }

    /// Get who last updated this entity
    pub fn updated_by(&self) -> Option<&Uuid> {
        self.metadata.updated_by.as_ref()
    }

    /// Get who deleted this entity
    pub fn deleted_by(&self) -> Option<&Uuid> {
        self.metadata.deleted_by.as_ref()
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the maximum_leave field (chainable)
    pub fn with_maximum_leave(mut self, value: Decimal) -> Self {
        self.maximum_leave = Some(value);
        self
    }

    /// Set the postponed_max_days field (chainable)
    pub fn with_postponed_max_days(mut self, value: Decimal) -> Self {
        self.postponed_max_days = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "plan_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.plan_id = v; }
                }
                "sequence" => {
                    if let Ok(v) = serde_json::from_value(value) { self.sequence = v; }
                }
                "start_count" => {
                    if let Ok(v) = serde_json::from_value(value) { self.start_count = v; }
                }
                "start_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.start_type = v; }
                }
                "frequency" => {
                    if let Ok(v) = serde_json::from_value(value) { self.frequency = v; }
                }
                "added_value" => {
                    if let Ok(v) = serde_json::from_value(value) { self.added_value = v; }
                }
                "is_added_based_on_worked_time" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_added_based_on_worked_time = v; }
                }
                "maximum_leave" => {
                    if let Ok(v) = serde_json::from_value(value) { self.maximum_leave = v; }
                }
                "action_with_lost_days" => {
                    if let Ok(v) = serde_json::from_value(value) { self.action_with_lost_days = v; }
                }
                "postponed_max_days" => {
                    if let Ok(v) = serde_json::from_value(value) { self.postponed_max_days = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for TimeoffAccrualLevel {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "TimeoffAccrualLevel"
    }
}

impl backbone_core::PersistentEntity for TimeoffAccrualLevel {
    fn entity_id(&self) -> String {
        self.id.to_string()
    }
    fn set_entity_id(&mut self, id: String) {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
            self.id = uuid;
        }
    }
    fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.created_at
    }
    fn set_created_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.created_at = Some(ts);
    }
    fn updated_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.updated_at
    }
    fn set_updated_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.updated_at = Some(ts);
    }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.deleted_at
    }
    fn set_deleted_at(&mut self, ts: Option<chrono::DateTime<chrono::Utc>>) {
        self.metadata.deleted_at = ts;
    }
}

impl backbone_orm::EntityRepoMeta for TimeoffAccrualLevel {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("plan_id".to_string(), "uuid".to_string());
        m.insert("start_type".to_string(), "accrual_start_type".to_string());
        m.insert("frequency".to_string(), "accrual_frequency".to_string());
        m.insert("action_with_lost_days".to_string(), "accrual_lost_days_action".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for TimeoffAccrualLevel entity
///
/// Provides a fluent API for constructing TimeoffAccrualLevel instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct TimeoffAccrualLevelBuilder {
    plan_id: Option<Uuid>,
    sequence: Option<i32>,
    start_count: Option<Decimal>,
    start_type: Option<AccrualStartType>,
    frequency: Option<AccrualFrequency>,
    added_value: Option<Decimal>,
    is_added_based_on_worked_time: Option<bool>,
    maximum_leave: Option<Decimal>,
    action_with_lost_days: Option<AccrualLostDaysAction>,
    postponed_max_days: Option<Decimal>,
}

impl TimeoffAccrualLevelBuilder {
    /// Set the plan_id field (required)
    pub fn plan_id(mut self, value: Uuid) -> Self {
        self.plan_id = Some(value);
        self
    }

    /// Set the sequence field (required)
    pub fn sequence(mut self, value: i32) -> Self {
        self.sequence = Some(value);
        self
    }

    /// Set the start_count field (default: `Decimal::from(0)`)
    pub fn start_count(mut self, value: Decimal) -> Self {
        self.start_count = Some(value);
        self
    }

    /// Set the start_type field (default: `AccrualStartType::default()`)
    pub fn start_type(mut self, value: AccrualStartType) -> Self {
        self.start_type = Some(value);
        self
    }

    /// Set the frequency field (default: `AccrualFrequency::default()`)
    pub fn frequency(mut self, value: AccrualFrequency) -> Self {
        self.frequency = Some(value);
        self
    }

    /// Set the added_value field (required)
    pub fn added_value(mut self, value: Decimal) -> Self {
        self.added_value = Some(value);
        self
    }

    /// Set the is_added_based_on_worked_time field (default: `false`)
    pub fn is_added_based_on_worked_time(mut self, value: bool) -> Self {
        self.is_added_based_on_worked_time = Some(value);
        self
    }

    /// Set the maximum_leave field (optional)
    pub fn maximum_leave(mut self, value: Decimal) -> Self {
        self.maximum_leave = Some(value);
        self
    }

    /// Set the action_with_lost_days field (default: `AccrualLostDaysAction::default()`)
    pub fn action_with_lost_days(mut self, value: AccrualLostDaysAction) -> Self {
        self.action_with_lost_days = Some(value);
        self
    }

    /// Set the postponed_max_days field (optional)
    pub fn postponed_max_days(mut self, value: Decimal) -> Self {
        self.postponed_max_days = Some(value);
        self
    }

    /// Build the TimeoffAccrualLevel entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<TimeoffAccrualLevel, String> {
        let plan_id = self.plan_id.ok_or_else(|| "plan_id is required".to_string())?;
        let sequence = self.sequence.ok_or_else(|| "sequence is required".to_string())?;
        let added_value = self.added_value.ok_or_else(|| "added_value is required".to_string())?;

        Ok(TimeoffAccrualLevel {
            id: Uuid::new_v4(),
            plan_id,
            sequence,
            start_count: self.start_count.unwrap_or(Decimal::from(0)),
            start_type: self.start_type.unwrap_or_default(),
            frequency: self.frequency.unwrap_or_default(),
            added_value,
            is_added_based_on_worked_time: self.is_added_based_on_worked_time.unwrap_or(false),
            maximum_leave: self.maximum_leave,
            action_with_lost_days: self.action_with_lost_days.unwrap_or_default(),
            postponed_max_days: self.postponed_max_days,
            metadata: AuditMetadata::default(),
        })
    }
}
