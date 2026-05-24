//! Core personal-memory engine for Mneme.
//!
//! `mneme-core` is the product runtime crate. It owns raw event capture, memory
//! claims, provenance, context-pack retrieval, budget checks, local store
//! adapters, extraction adapter boundaries, and agent session records.
//!
//! The primary public entry point is [`MnemeEngine`]. Most integrations should
//! construct an engine with [`MnemeConfig`], append [`EventInput`] records,
//! retrieve a [`ContextPack`] through a scoped [`ContextQuery`], and persist
//! state through a [`MnemeStore`] implementation such as [`JsonFileStore`] or
//! [`InMemoryStore`].
//!
//! Mneme is pre-1.0, so Rust type names can still change. The intended current
//! extension points are [`MnemeStore`] for persistence and [`MnemeExtractor`] for
//! claim extraction. Behavior changes should remain covered by specs, tests,
//! eval scenarios, and the release quality gate.
//!
//! # Example
//!
//! ```
//! use mneme_core::{EventInput, MnemeConfig, MnemeEngine};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut engine = MnemeEngine::new(MnemeConfig::default());
//! engine.ingest_event(EventInput {
//!     speaker_id: "user".to_owned(),
//!     actor_agent_id: Some("codex".to_owned()),
//!     text: "remember: user prefers local-first tools".to_owned(),
//!     scope: "private".to_owned(),
//!     trust_level: "explicit".to_owned(),
//! })?;
//!
//! let context = engine.build_context_pack("local-first");
//! assert_eq!(context.items[0].claim_text, "user prefers local-first tools");
//! # Ok(())
//! # }
//! ```

mod v1;

pub use v1::{
    validate_state, AuditKind, AuditRecord, BudgetState, ClaimRecord, ClaimStatus,
    CommandExtractor, CompactionReport, ContextItem, ContextPack, ContextQuery, EngineSnapshot,
    EventInput, EventRecord, ExtractedClaim, ExtractorCommandRequest, ExtractorCommandResponse,
    ExtractorError, InMemoryStore, JsonFileStore, MigrationRecord, MnemeConfig, MnemeEngine,
    MnemeExtractor, MnemeState, MnemeStore, OmittedContextItem, RuleBasedExtractor,
    SessionBeginInput, SessionBeginReport, SessionEndInput, SessionEndReport, SessionError,
    SessionRecord, SessionStatus, StateMetadata, StateValidationIssue, StateValidationReport,
    StoreError, StoreErrorKind, StoreFileInspection, StoreFileStatus, StoreInspection,
    StoreRepairReport, StoreRestoreReport, ValidationSeverity, DEFAULT_CONTEXT_MAX_ITEMS,
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
