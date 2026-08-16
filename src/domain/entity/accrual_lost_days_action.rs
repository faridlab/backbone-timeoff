use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "accrual_lost_days_action", rename_all = "snake_case")]
pub enum AccrualLostDaysAction {
    Nothing,
    PostponedToNextAccrual,
}

impl std::fmt::Display for AccrualLostDaysAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nothing => write!(f, "nothing"),
            Self::PostponedToNextAccrual => write!(f, "postponed_to_next_accrual"),
        }
    }
}

impl FromStr for AccrualLostDaysAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "nothing" => Ok(Self::Nothing),
            "postponed_to_next_accrual" => Ok(Self::PostponedToNextAccrual),
            _ => Err(format!("Unknown AccrualLostDaysAction variant: {}", s)),
        }
    }
}

impl Default for AccrualLostDaysAction {
    fn default() -> Self {
        Self::Nothing
    }
}
