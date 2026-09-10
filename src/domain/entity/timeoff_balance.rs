use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use rust_decimal::Decimal;
use super::AuditMetadata;

/// Strongly-typed ID for TimeoffBalance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimeoffBalanceId(pub Uuid);

impl TimeoffBalanceId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for TimeoffBalanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for TimeoffBalanceId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for TimeoffBalanceId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<TimeoffBalanceId> for Uuid {
    fn from(id: TimeoffBalanceId) -> Self { id.0 }
}

impl AsRef<Uuid> for TimeoffBalanceId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for TimeoffBalanceId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TimeoffBalance {
    pub id: Uuid,
    pub timeoff_type_id: Uuid,
    pub employee_id: Uuid,
    pub period: String,
    pub allocated: Decimal,
    pub used: Decimal,
    pub accrual_plan_id: Option<Uuid>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub last_accrual_at: Option<DateTime<Utc>>,
    pub carried_over: Decimal,
    pub expired_at: Option<DateTime<Utc>>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl TimeoffBalance {
    /// Create a builder for TimeoffBalance
    pub fn builder() -> TimeoffBalanceBuilder {
        <TimeoffBalanceBuilder as Default>::default()
    }

    /// Create a new TimeoffBalance with required fields
    pub fn new(timeoff_type_id: Uuid, employee_id: Uuid, period: String, allocated: Decimal, used: Decimal, carried_over: Decimal) -> Self {
        Self {
            id: Uuid::new_v4(),
            timeoff_type_id,
            employee_id,
            period,
            allocated,
            used,
            accrual_plan_id: None,
            date_from: None,
            date_to: None,
            last_accrual_at: None,
            carried_over,
            expired_at: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> TimeoffBalanceId {
        TimeoffBalanceId(self.id)
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

    /// Set the accrual_plan_id field (chainable)
    pub fn with_accrual_plan_id(mut self, value: Uuid) -> Self {
        self.accrual_plan_id = Some(value);
        self
    }

    /// Set the date_from field (chainable)
    pub fn with_date_from(mut self, value: NaiveDate) -> Self {
        self.date_from = Some(value);
        self
    }

    /// Set the date_to field (chainable)
    pub fn with_date_to(mut self, value: NaiveDate) -> Self {
        self.date_to = Some(value);
        self
    }

    /// Set the last_accrual_at field (chainable)
    pub fn with_last_accrual_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_accrual_at = Some(value);
        self
    }

    /// Set the expired_at field (chainable)
    pub fn with_expired_at(mut self, value: DateTime<Utc>) -> Self {
        self.expired_at = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "timeoff_type_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.timeoff_type_id = v; }
                }
                "employee_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.employee_id = v; }
                }
                "period" => {
                    if let Ok(v) = serde_json::from_value(value) { self.period = v; }
                }
                "allocated" => {
                    if let Ok(v) = serde_json::from_value(value) { self.allocated = v; }
                }
                "used" => {
                    if let Ok(v) = serde_json::from_value(value) { self.used = v; }
                }
                "accrual_plan_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.accrual_plan_id = v; }
                }
                "date_from" => {
                    if let Ok(v) = serde_json::from_value(value) { self.date_from = v; }
                }
                "date_to" => {
                    if let Ok(v) = serde_json::from_value(value) { self.date_to = v; }
                }
                "last_accrual_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.last_accrual_at = v; }
                }
                "carried_over" => {
                    if let Ok(v) = serde_json::from_value(value) { self.carried_over = v; }
                }
                "expired_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.expired_at = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for TimeoffBalance {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "TimeoffBalance"
    }
}

impl backbone_core::PersistentEntity for TimeoffBalance {
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

impl backbone_orm::EntityRepoMeta for TimeoffBalance {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("timeoff_type_id".to_string(), "uuid".to_string());
        m.insert("employee_id".to_string(), "uuid".to_string());
        m.insert("accrual_plan_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["period"]
    }
}

/// Builder for TimeoffBalance entity
///
/// Provides a fluent API for constructing TimeoffBalance instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct TimeoffBalanceBuilder {
    timeoff_type_id: Option<Uuid>,
    employee_id: Option<Uuid>,
    period: Option<String>,
    allocated: Option<Decimal>,
    used: Option<Decimal>,
    accrual_plan_id: Option<Uuid>,
    date_from: Option<NaiveDate>,
    date_to: Option<NaiveDate>,
    last_accrual_at: Option<DateTime<Utc>>,
    carried_over: Option<Decimal>,
    expired_at: Option<DateTime<Utc>>,
}

impl TimeoffBalanceBuilder {
    /// Set the timeoff_type_id field (required)
    pub fn timeoff_type_id(mut self, value: Uuid) -> Self {
        self.timeoff_type_id = Some(value);
        self
    }

    /// Set the employee_id field (required)
    pub fn employee_id(mut self, value: Uuid) -> Self {
        self.employee_id = Some(value);
        self
    }

    /// Set the period field (required)
    pub fn period(mut self, value: String) -> Self {
        self.period = Some(value);
        self
    }

    /// Set the allocated field (required)
    pub fn allocated(mut self, value: Decimal) -> Self {
        self.allocated = Some(value);
        self
    }

    /// Set the used field (default: `Decimal::from(0)`)
    pub fn used(mut self, value: Decimal) -> Self {
        self.used = Some(value);
        self
    }

    /// Set the accrual_plan_id field (optional)
    pub fn accrual_plan_id(mut self, value: Uuid) -> Self {
        self.accrual_plan_id = Some(value);
        self
    }

    /// Set the date_from field (optional)
    pub fn date_from(mut self, value: NaiveDate) -> Self {
        self.date_from = Some(value);
        self
    }

    /// Set the date_to field (optional)
    pub fn date_to(mut self, value: NaiveDate) -> Self {
        self.date_to = Some(value);
        self
    }

    /// Set the last_accrual_at field (optional)
    pub fn last_accrual_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_accrual_at = Some(value);
        self
    }

    /// Set the carried_over field (default: `Decimal::from(0)`)
    pub fn carried_over(mut self, value: Decimal) -> Self {
        self.carried_over = Some(value);
        self
    }

    /// Set the expired_at field (optional)
    pub fn expired_at(mut self, value: DateTime<Utc>) -> Self {
        self.expired_at = Some(value);
        self
    }

    /// Build the TimeoffBalance entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<TimeoffBalance, String> {
        let timeoff_type_id = self.timeoff_type_id.ok_or_else(|| "timeoff_type_id is required".to_string())?;
        let employee_id = self.employee_id.ok_or_else(|| "employee_id is required".to_string())?;
        let period = self.period.ok_or_else(|| "period is required".to_string())?;
        let allocated = self.allocated.ok_or_else(|| "allocated is required".to_string())?;

        Ok(TimeoffBalance {
            id: Uuid::new_v4(),
            timeoff_type_id,
            employee_id,
            period,
            allocated,
            used: self.used.unwrap_or(Decimal::from(0)),
            accrual_plan_id: self.accrual_plan_id,
            date_from: self.date_from,
            date_to: self.date_to,
            last_accrual_at: self.last_accrual_at,
            carried_over: self.carried_over.unwrap_or(Decimal::from(0)),
            expired_at: self.expired_at,
            metadata: AuditMetadata::default(),
        })
    }
}
