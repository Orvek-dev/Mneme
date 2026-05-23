//! Shared core types and constants for Mneme.
//!
//! This crate starts intentionally small. Eval Harness v0 should define the
//! first executable contracts before product runtime behavior grows here.

/// Public product name.
pub const PRODUCT_NAME: &str = "Mneme";

/// Current repository bootstrap stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStage {
    /// Repository scaffold is ready; Eval Harness v0 is next.
    Bootstrap,
}

impl BuildStage {
    /// Returns the stable identifier used in setup checks.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_bootstrap_stage() {
        assert_eq!(PRODUCT_NAME, "Mneme");
        assert_eq!(BuildStage::Bootstrap.as_str(), "bootstrap");
    }
}
