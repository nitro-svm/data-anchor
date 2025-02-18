use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The percentile of recent prioritization fees to use as the compute unit price for a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub enum Priority {
    /// 0th percentile
    Min,
    /// 25th percentile
    Low,
    /// 50th percentile
    #[default]
    Medium,
    /// 75th percentile
    High,
    /// 95th percentile
    VeryHigh,
}

impl Priority {
    /// Converts the priority enumeration to a percentile value between 0 and 1.
    pub fn percentile(&self) -> f32 {
        match self {
            Self::Min => 0.0,
            Self::Low => 0.25,
            Self::Medium => 0.5,
            Self::High => 0.75,
            Self::VeryHigh => 0.95,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medium_is_default() {
        // This is probably important enough to warrant locking it down with a test.
        let default = Priority::default();
        let medium = Priority::Medium;
        assert_eq!(medium, default);
    }
}
