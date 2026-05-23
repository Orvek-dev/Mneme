use mneme_core::{EventInput, MnemeConfig, MnemeEngine};

use crate::error::EvalError;
use crate::scenario::Scenario;
use crate::target::{
    ActualState, AuditEvent, BudgetActual, Claim, ContextItem, ContextPack, EvalTarget, FaultMode,
    OmittedItem, RecordedEvent, TargetRunOptions,
};

pub(crate) struct MnemeV1EvalTarget;

impl EvalTarget for MnemeV1EvalTarget {
    fn name(&self) -> &'static str {
        "mneme-v1"
    }

    fn run(
        &self,
        scenario: &Scenario,
        options: TargetRunOptions,
    ) -> Result<ActualState, EvalError> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: scenario.budget.daily_cloud_tokens,
        });
        for input in &scenario.events {
            engine.ingest_event(EventInput {
                speaker_id: input.speaker_id.clone(),
                actor_agent_id: input.actor_agent_id.clone(),
                text: input.text.clone(),
                scope: input.scope.clone(),
                trust_level: input.trust_level.clone(),
            });
        }

        let context_pack = scenario
            .expected
            .context_pack
            .as_ref()
            .map(|expected| engine.build_context_pack(expected.query.clone()));
        let snapshot = engine.snapshot();
        let mut actual = ActualState {
            events: snapshot
                .events
                .into_iter()
                .map(|event| RecordedEvent {
                    id: event.id,
                    speaker_id: event.speaker_id,
                    actor_agent_id: event.actor_agent_id,
                    text: event.text,
                    scope: event.scope,
                    trust_level: event.trust_level,
                })
                .collect(),
            claims: snapshot
                .claims
                .into_iter()
                .map(|claim| Claim {
                    id: claim.id,
                    subject: claim.subject,
                    predicate: claim.predicate,
                    object: claim.object,
                    status: claim.status.as_str().to_owned(),
                    scope: claim.scope,
                    source_event_ids: claim.source_event_ids,
                })
                .collect(),
            context_pack: context_pack.map(|pack| ContextPack {
                items: pack
                    .items
                    .into_iter()
                    .map(|item| ContextItem {
                        claim_id: item.claim_id,
                        claim_text: item.claim_text,
                        source_event_ids: item.source_event_ids,
                    })
                    .collect(),
                omitted: pack
                    .omitted
                    .into_iter()
                    .map(|item| OmittedItem {
                        claim_id: item.claim_id,
                        reason: item.reason,
                    })
                    .collect(),
            }),
            budget: BudgetActual {
                spent_tokens: snapshot.budget.spent_tokens,
                hard_cap_violations: snapshot.budget.hard_cap_violations,
            },
            audit: snapshot
                .audit
                .into_iter()
                .map(|event| AuditEvent {
                    kind: event.kind.as_str().to_owned(),
                    target_id: event.target_id,
                })
                .collect(),
        };
        apply_seeded_fault(&mut actual, options.fault_mode);
        Ok(actual)
    }
}

fn apply_seeded_fault(actual: &mut ActualState, fault_mode: FaultMode) {
    match fault_mode {
        FaultMode::None => {}
        FaultMode::SkipClaims => {
            actual.claims.clear();
            if let Some(context_pack) = &mut actual.context_pack {
                context_pack.items.clear();
            }
            actual.audit.retain(|event| event.kind != "claim.write");
        }
        FaultMode::LeakSecrets => {
            for claim in &mut actual.claims {
                if claim.status == "blocked_secret" {
                    claim.status = "active".to_owned();
                }
            }
        }
        FaultMode::DropCitations => {
            if let Some(context_pack) = &mut actual.context_pack {
                for item in &mut context_pack.items {
                    item.source_event_ids.clear();
                }
            }
        }
    }
}
