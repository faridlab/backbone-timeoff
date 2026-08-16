use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::TimeoffRequestStatus;
use super::AuditMetadata;

/// Strongly-typed ID for TimeoffRequest
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimeoffRequestId(pub Uuid);

impl TimeoffRequestId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for TimeoffRequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for TimeoffRequestId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for TimeoffRequestId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<TimeoffRequestId> for Uuid {
    fn from(id: TimeoffRequestId) -> Self { id.0 }
}

impl AsRef<Uuid> for TimeoffRequestId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for TimeoffRequestId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TimeoffRequest {
    pub id: Uuid,
    pub company_id: Uuid,
    pub timeoff_type_id: Uuid,
    pub employee_id: Uuid,
    pub date_start: NaiveDate,
    pub date_end: NaiveDate,
    pub note: Option<String>,
    pub approval_employee_id: Option<Uuid>,
    pub approval_request_id: Option<Uuid>,
    pub note_reject: Option<String>,
    pub status: TimeoffRequestStatus,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl TimeoffRequest {
    /// Create a builder for TimeoffRequest
    pub fn builder() -> TimeoffRequestBuilder {
        <TimeoffRequestBuilder as Default>::default()
    }

    /// Create a new TimeoffRequest with required fields
    pub fn new(company_id: Uuid, timeoff_type_id: Uuid, employee_id: Uuid, date_start: NaiveDate, date_end: NaiveDate, status: TimeoffRequestStatus) -> Self {
        Self {
            id: Uuid::new_v4(),
            company_id,
            timeoff_type_id,
            employee_id,
            date_start,
            date_end,
            note: None,
            approval_employee_id: None,
            approval_request_id: None,
            note_reject: None,
            status,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> TimeoffRequestId {
        TimeoffRequestId(self.id)
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

    /// Get the current status
    pub fn status(&self) -> &TimeoffRequestStatus {
        &self.status
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the note field (chainable)
    pub fn with_note(mut self, value: String) -> Self {
        self.note = Some(value);
        self
    }

    /// Set the approval_employee_id field (chainable)
    pub fn with_approval_employee_id(mut self, value: Uuid) -> Self {
        self.approval_employee_id = Some(value);
        self
    }

    /// Set the approval_request_id field (chainable)
    pub fn with_approval_request_id(mut self, value: Uuid) -> Self {
        self.approval_request_id = Some(value);
        self
    }

    /// Set the note_reject field (chainable)
    pub fn with_note_reject(mut self, value: String) -> Self {
        self.note_reject = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "company_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.company_id = v; }
                }
                "timeoff_type_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.timeoff_type_id = v; }
                }
                "employee_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.employee_id = v; }
                }
                "date_start" => {
                    if let Ok(v) = serde_json::from_value(value) { self.date_start = v; }
                }
                "date_end" => {
                    if let Ok(v) = serde_json::from_value(value) { self.date_end = v; }
                }
                "note" => {
                    if let Ok(v) = serde_json::from_value(value) { self.note = v; }
                }
                "approval_employee_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.approval_employee_id = v; }
                }
                "approval_request_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.approval_request_id = v; }
                }
                "note_reject" => {
                    if let Ok(v) = serde_json::from_value(value) { self.note_reject = v; }
                }
                "status" => {
                    if let Ok(v) = serde_json::from_value(value) { self.status = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for TimeoffRequest {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "TimeoffRequest"
    }
}

impl backbone_core::PersistentEntity for TimeoffRequest {
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

impl backbone_orm::EntityRepoMeta for TimeoffRequest {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert("timeoff_type_id".to_string(), "uuid".to_string());
        m.insert("employee_id".to_string(), "uuid".to_string());
        m.insert("approval_employee_id".to_string(), "uuid".to_string());
        m.insert("approval_request_id".to_string(), "uuid".to_string());
        m.insert("status".to_string(), "timeoff_request_status".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
}

/// Builder for TimeoffRequest entity
///
/// Provides a fluent API for constructing TimeoffRequest instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct TimeoffRequestBuilder {
    company_id: Option<Uuid>,
    timeoff_type_id: Option<Uuid>,
    employee_id: Option<Uuid>,
    date_start: Option<NaiveDate>,
    date_end: Option<NaiveDate>,
    note: Option<String>,
    approval_employee_id: Option<Uuid>,
    approval_request_id: Option<Uuid>,
    note_reject: Option<String>,
    status: Option<TimeoffRequestStatus>,
}

impl TimeoffRequestBuilder {
    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

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

    /// Set the date_start field (required)
    pub fn date_start(mut self, value: NaiveDate) -> Self {
        self.date_start = Some(value);
        self
    }

    /// Set the date_end field (required)
    pub fn date_end(mut self, value: NaiveDate) -> Self {
        self.date_end = Some(value);
        self
    }

    /// Set the note field (optional)
    pub fn note(mut self, value: String) -> Self {
        self.note = Some(value);
        self
    }

    /// Set the approval_employee_id field (optional)
    pub fn approval_employee_id(mut self, value: Uuid) -> Self {
        self.approval_employee_id = Some(value);
        self
    }

    /// Set the approval_request_id field (optional)
    pub fn approval_request_id(mut self, value: Uuid) -> Self {
        self.approval_request_id = Some(value);
        self
    }

    /// Set the note_reject field (optional)
    pub fn note_reject(mut self, value: String) -> Self {
        self.note_reject = Some(value);
        self
    }

    /// Set the status field (default: `TimeoffRequestStatus::default()`)
    pub fn status(mut self, value: TimeoffRequestStatus) -> Self {
        self.status = Some(value);
        self
    }

    /// Build the TimeoffRequest entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<TimeoffRequest, String> {
        let company_id = self.company_id.ok_or_else(|| "company_id is required".to_string())?;
        let timeoff_type_id = self.timeoff_type_id.ok_or_else(|| "timeoff_type_id is required".to_string())?;
        let employee_id = self.employee_id.ok_or_else(|| "employee_id is required".to_string())?;
        let date_start = self.date_start.ok_or_else(|| "date_start is required".to_string())?;
        let date_end = self.date_end.ok_or_else(|| "date_end is required".to_string())?;

        Ok(TimeoffRequest {
            id: Uuid::new_v4(),
            company_id,
            timeoff_type_id,
            employee_id,
            date_start,
            date_end,
            note: self.note,
            approval_employee_id: self.approval_employee_id,
            approval_request_id: self.approval_request_id,
            note_reject: self.note_reject,
            status: self.status.unwrap_or_default(),
            metadata: AuditMetadata::default(),
        })
    }
}
