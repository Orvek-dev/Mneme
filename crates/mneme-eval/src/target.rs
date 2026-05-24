use std::collections::BTreeMap;

use crate::error::EvalError;
use crate::fake::FakeEvalTarget;
use crate::mneme_v1::{MnemeV1CommandEvalTarget, MnemeV1EvalTarget};
use crate::scenario::Scenario;
use serde::{Deserialize, Serialize};

pub(crate) trait EvalTarget {
    fn name(&self) -> &'static str;

    fn metadata(&self, options: &TargetRunOptions) -> EvalTargetMetadata;

    fn run(&self, scenario: &Scenario, options: TargetRunOptions)
        -> Result<ActualState, EvalError>;
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum TargetKind {
    Fake,
    MnemeV1,
    MnemeV1Command,
}

impl TargetKind {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "fake" => Some(Self::Fake),
            "mneme-v1" => Some(Self::MnemeV1),
            "mneme-v1-command" => Some(Self::MnemeV1Command),
            _ => None,
        }
    }

    pub(crate) fn available() -> &'static str {
        "fake, mneme-v1, mneme-v1-command"
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Fake => "fake",
            Self::MnemeV1 => "mneme-v1",
            Self::MnemeV1Command => "mneme-v1-command",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum FaultMode {
    None,
    SkipClaims,
    LeakSecrets,
    DropCitations,
}

impl FaultMode {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "skip-claims" => Some(Self::SkipClaims),
            "leak-secrets" => Some(Self::LeakSecrets),
            "drop-citations" => Some(Self::DropCitations),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::SkipClaims => "skip-claims",
            Self::LeakSecrets => "leak-secrets",
            Self::DropCitations => "drop-citations",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TargetRunOptions {
    pub(crate) fault_mode: FaultMode,
    pub(crate) command_extractor: Option<CommandExtractorOptions>,
}

impl Default for TargetRunOptions {
    fn default() -> Self {
        Self {
            fault_mode: FaultMode::None,
            command_extractor: None,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CommandExtractorOptions {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct EvalTargetMetadata {
    pub(crate) extractor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) protocol: Option<String>,
    pub(crate) opt_in: bool,
    pub(crate) command_configured: bool,
}

impl EvalTargetMetadata {
    pub(crate) fn fake() -> Self {
        Self {
            extractor: "fixture".to_owned(),
            protocol: None,
            opt_in: false,
            command_configured: false,
        }
    }

    pub(crate) fn rule_based() -> Self {
        Self {
            extractor: "rule-based".to_owned(),
            protocol: None,
            opt_in: false,
            command_configured: false,
        }
    }

    pub(crate) fn command(configured: bool) -> Self {
        Self {
            extractor: "command".to_owned(),
            protocol: Some(mneme_core::EXTRACTOR_COMMAND_SCHEMA_VERSION.to_owned()),
            opt_in: true,
            command_configured: configured,
        }
    }
}

pub(crate) fn build_target(kind: TargetKind) -> Box<dyn EvalTarget> {
    match kind {
        TargetKind::Fake => Box::new(FakeEvalTarget),
        TargetKind::MnemeV1 => Box::new(MnemeV1EvalTarget),
        TargetKind::MnemeV1Command => Box::new(MnemeV1CommandEvalTarget),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RecordedEvent {
    pub(crate) id: String,
    pub(crate) speaker_id: String,
    pub(crate) actor_agent_id: Option<String>,
    pub(crate) text: String,
    pub(crate) scope: String,
    pub(crate) trust_level: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Claim {
    pub(crate) id: String,
    pub(crate) subject: String,
    pub(crate) predicate: String,
    pub(crate) object: String,
    pub(crate) status: String,
    pub(crate) scope: String,
    pub(crate) source_event_ids: Vec<String>,
}

impl Claim {
    pub(crate) fn text(&self) -> String {
        format!("{} {} {}", self.subject, self.predicate, self.object)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ContextPack {
    pub(crate) items: Vec<ContextItem>,
    pub(crate) omitted: Vec<OmittedItem>,
}

#[derive(Debug, Clone)]
pub(crate) struct ContextItem {
    pub(crate) claim_id: String,
    pub(crate) claim_text: String,
    pub(crate) source_event_ids: Vec<String>,
    pub(crate) score: u32,
    pub(crate) matched_terms: Vec<String>,
    pub(crate) match_reason: String,
}

#[derive(Debug, Clone)]
pub(crate) struct OmittedItem {
    pub(crate) claim_id: String,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BudgetActual {
    pub(crate) spent_tokens: u32,
    pub(crate) hard_cap_violations: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct AuditEvent {
    pub(crate) kind: String,
    pub(crate) target_id: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ActualState {
    pub(crate) events: Vec<RecordedEvent>,
    pub(crate) claims: Vec<Claim>,
    pub(crate) sessions: Vec<SessionActual>,
    pub(crate) context_pack: Option<ContextPack>,
    pub(crate) budget: BudgetActual,
    pub(crate) audit: Vec<AuditEvent>,
    pub(crate) store: Option<StoreActual>,
    pub(crate) quality: Option<QualityActual>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionActual {
    pub(crate) id: String,
    pub(crate) task: String,
    pub(crate) actor_agent_id: Option<String>,
    pub(crate) status: String,
    pub(crate) context_claim_ids: Vec<String>,
    pub(crate) summary: Option<String>,
    pub(crate) memory_event_ids: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct StoreActual {
    pub(crate) schema_version: Option<u32>,
    pub(crate) valid: bool,
    pub(crate) backup_present: bool,
    pub(crate) repair_performed: bool,
    pub(crate) compacted: bool,
    pub(crate) imported: bool,
    pub(crate) generation: Option<u64>,
    pub(crate) error_count: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct QualityActual {
    pub(crate) duplicate_active_groups: usize,
    pub(crate) duplicate_active_claims: usize,
    pub(crate) blocked_secret_count: usize,
    pub(crate) inactive_claim_count: usize,
    pub(crate) review_item_count: usize,
    pub(crate) finding_kinds: Vec<String>,
}

pub(crate) fn build_quality_actual(claims: &[Claim]) -> QualityActual {
    let mut active_groups = BTreeMap::<String, usize>::new();
    let mut blocked_secret_count = 0;
    let mut inactive_claim_count = 0;
    for claim in claims {
        match claim.status.as_str() {
            "active" => {
                *active_groups.entry(quality_claim_key(claim)).or_default() += 1;
            }
            "blocked_secret" => blocked_secret_count += 1,
            "superseded" | "forgotten" => inactive_claim_count += 1,
            _ => {}
        }
    }
    let duplicate_active_groups = active_groups.values().filter(|count| **count > 1).count();
    let duplicate_active_claims = active_groups
        .values()
        .filter(|count| **count > 1)
        .sum::<usize>();
    let mut finding_kinds = Vec::new();
    if duplicate_active_groups > 0 {
        finding_kinds.push("duplicate_active".to_owned());
    }
    if blocked_secret_count > 0 {
        finding_kinds.push("blocked_secret".to_owned());
    }
    if inactive_claim_count > 0 {
        finding_kinds.push("inactive_history".to_owned());
    }
    let review_item_count =
        duplicate_active_groups + blocked_secret_count + usize::from(inactive_claim_count > 0);
    QualityActual {
        duplicate_active_groups,
        duplicate_active_claims,
        blocked_secret_count,
        inactive_claim_count,
        review_item_count,
        finding_kinds,
    }
}

fn quality_claim_key(claim: &Claim) -> String {
    [
        normalize_quality_value(&claim.subject),
        normalize_quality_value(&claim.predicate),
        normalize_quality_value(&claim.object),
        normalize_quality_value(&claim.scope),
    ]
    .join("\u{1f}")
}

fn normalize_quality_value(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}
