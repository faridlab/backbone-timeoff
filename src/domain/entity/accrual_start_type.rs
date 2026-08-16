use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "accrual_start_type", rename_all = "snake_case")]
pub enum AccrualStartType {
    Days,
    Months,
    Years,
}

impl std::fmt::Display for AccrualStartType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Days => write!(f, "days"),
            Self::Months => write!(f, "months"),
            Self::Years => write!(f, "years"),
        }
    }
}

impl FromStr for AccrualStartType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "days" => Ok(Self::Days),
            "months" => Ok(Self::Months),
            "years" => Ok(Self::Years),
            _ => Err(format!("Unknown AccrualStartType variant: {}", s)),
        }
    }
}

impl Default for AccrualStartType {
    fn default() -> Self {
        Self::Years
    }
}
