use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "accrual_frequency", rename_all = "snake_case")]
pub enum AccrualFrequency {
    Daily,
    Weekly,
    Biweekly,
    Monthly,
    Bimonthly,
    Quarterly,
    Yearly,
    Once,
}

impl std::fmt::Display for AccrualFrequency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Daily => write!(f, "daily"),
            Self::Weekly => write!(f, "weekly"),
            Self::Biweekly => write!(f, "biweekly"),
            Self::Monthly => write!(f, "monthly"),
            Self::Bimonthly => write!(f, "bimonthly"),
            Self::Quarterly => write!(f, "quarterly"),
            Self::Yearly => write!(f, "yearly"),
            Self::Once => write!(f, "once"),
        }
    }
}

impl FromStr for AccrualFrequency {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            "biweekly" => Ok(Self::Biweekly),
            "monthly" => Ok(Self::Monthly),
            "bimonthly" => Ok(Self::Bimonthly),
            "quarterly" => Ok(Self::Quarterly),
            "yearly" => Ok(Self::Yearly),
            "once" => Ok(Self::Once),
            _ => Err(format!("Unknown AccrualFrequency variant: {}", s)),
        }
    }
}

impl Default for AccrualFrequency {
    fn default() -> Self {
        Self::Monthly
    }
}
