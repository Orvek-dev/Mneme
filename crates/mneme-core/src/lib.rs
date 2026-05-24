//! Shared core types and constants for Mneme.
//!
//! This crate starts intentionally small. Eval Harness v0 should define the
//! first executable contracts before product runtime behavior grows here.

mod v1;

pub use v1::{
    validate_state, AuditKind, AuditRecord, BudgetState, ClaimRecord, ClaimStatus,
    CommandExtractor, CompactionReport, ContextItem, ContextPack, EngineSnapshot, EventInput,
    EventRecord, ExtractedClaim, ExtractorCommandRequest, ExtractorCommandResponse, ExtractorError,
    InMemoryStore, JsonFileStore, MigrationRecord, MnemeConfig, MnemeEngine, MnemeExtractor,
    MnemeState, MnemeStore, OmittedContextItem, RuleBasedExtractor, SessionBeginInput,
    SessionBeginReport, SessionEndInput, SessionEndReport, SessionError, SessionRecord,
    SessionStatus, StateMetadata, StateValidationIssue, StateValidationReport, StoreError,
    StoreFileInspection, StoreFileStatus, StoreInspection, StoreRepairReport, ValidationSeverity,
    EXTRACTOR_COMMAND_SCHEMA_VERSION, MNEME_STATE_SCHEMA_VERSION,
};

/// Public product name.
pub const PRODUCT_NAME: &str = "Mneme";

/// Current repository bootstrap stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStage {
    /// Repository scaffold is ready; Eval Harness v0 is next.
    Bootstrap,
    /// Personal-memory v1 core is available behind eval harness gates.
    PersonalCoreV1,
}

impl BuildStage {
    /// Returns the stable identifier used in setup checks.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::PersonalCoreV1 => "personal-core-v1",
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
        assert_eq!(BuildStage::PersonalCoreV1.as_str(), "personal-core-v1");
    }
}
