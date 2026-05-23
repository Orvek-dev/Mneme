use crate::error::EvalError;
use crate::fake::FakeEvalTarget;
use crate::scenario::Scenario;

pub(crate) trait EvalTarget {
    fn name(&self) -> &'static str;

    fn run(&self, scenario: &Scenario, options: TargetRunOptions)
        -> Result<ActualState, EvalError>;
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum TargetKind {
    Fake,
}

impl TargetKind {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "fake" => Some(Self::Fake),
            _ => None,
        }
    }

    pub(crate) fn available() -> &'static str {
        "fake"
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Fake => "fake",
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct TargetRunOptions {
    pub(crate) fault_mode: FaultMode,
}

impl Default for TargetRunOptions {
    fn default() -> Self {
        Self {
            fault_mode: FaultMode::None,
        }
    }
}

pub(crate) fn build_target(kind: TargetKind) -> Box<dyn EvalTarget> {
    match kind {
        TargetKind::Fake => Box::new(FakeEvalTarget),
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
    pub(crate) context_pack: Option<ContextPack>,
    pub(crate) budget: BudgetActual,
    pub(crate) audit: Vec<AuditEvent>,
}
