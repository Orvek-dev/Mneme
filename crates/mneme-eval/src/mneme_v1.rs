use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mneme_core::{
    CommandExtractor, ContextQuery, EventInput, JsonFileStore, MnemeConfig, MnemeEngine,
    MnemeStore, SessionBeginInput, SessionEndInput, SessionMemoryInputMode, StoreFileStatus,
};

use crate::error::EvalError;
use crate::scenario::{AgentEndExtractor, Scenario};
use crate::target::{
    build_curation_actual, build_curation_plan_actual, build_quality_actual, ActualState,
    AuditEvent, BudgetActual, Claim, ContextItem, ContextPack, EvalTarget, EvalTargetMetadata,
    FaultMode, OmittedItem, RecordedEvent, SessionActual, StoreActual, TargetRunOptions,
};

pub(crate) struct MnemeV1EvalTarget;
pub(crate) struct MnemeV1CommandEvalTarget;

impl EvalTarget for MnemeV1EvalTarget {
    fn name(&self) -> &'static str {
        "mneme-v1"
    }

    fn metadata(&self, _options: &TargetRunOptions) -> EvalTargetMetadata {
        EvalTargetMetadata::rule_based()
    }

    fn run(
        &self,
        scenario: &Scenario,
        options: TargetRunOptions,
    ) -> Result<ActualState, EvalError> {
        let persistence_path = needs_store(scenario).then(|| temp_store_path(&scenario.id));
        let result = run_with_optional_persistence(
            scenario,
            options,
            persistence_path.as_deref(),
            ExtractorMode::Rule,
        );
        if let Some(path) = persistence_path {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(backup_path_for(&path));
        }
        result
    }
}

impl EvalTarget for MnemeV1CommandEvalTarget {
    fn name(&self) -> &'static str {
        "mneme-v1-command"
    }

    fn metadata(&self, options: &TargetRunOptions) -> EvalTargetMetadata {
        EvalTargetMetadata::command(options.command_extractor.is_some())
    }

    fn run(
        &self,
        scenario: &Scenario,
        options: TargetRunOptions,
    ) -> Result<ActualState, EvalError> {
        let persistence_path = needs_store(scenario).then(|| temp_store_path(&scenario.id));
        let result = run_with_optional_persistence(
            scenario,
            options,
            persistence_path.as_deref(),
            ExtractorMode::Command,
        );
        if let Some(path) = persistence_path {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(backup_path_for(&path));
        }
        result
    }
}

#[derive(Debug, Clone, Copy)]
enum ExtractorMode {
    Rule,
    Command,
}

fn run_with_optional_persistence(
    scenario: &Scenario,
    options: TargetRunOptions,
    persistence_path: Option<&Path>,
    extractor_mode: ExtractorMode,
) -> Result<ActualState, EvalError> {
    let config = MnemeConfig {
        daily_cloud_tokens: scenario.budget.daily_cloud_tokens,
    };
    let mut engine = MnemeEngine::new(config);
    let mut store_run = StoreRunState::default();
    let command_extractor = match extractor_mode {
        ExtractorMode::Rule => None,
        ExtractorMode::Command => {
            let command = options.command_extractor.as_ref().ok_or_else(|| {
                EvalError::invalid_cli("mneme-v1-command requires --extractor-command <program>")
            })?;
            Some(CommandExtractor::new(
                command.program.clone(),
                command.args.clone(),
            ))
        }
    };

    for (idx, input) in scenario.events.iter().enumerate() {
        let event = EventInput {
            speaker_id: input.speaker_id.clone(),
            actor_agent_id: input.actor_agent_id.clone(),
            text: input.text.clone(),
            scope: input.scope.clone(),
            trust_level: input.trust_level.clone(),
        };
        match &command_extractor {
            Some(extractor) => engine.ingest_event_with_extractor(event, extractor),
            None => engine.ingest_event(event),
        }
        .map_err(|source| {
            EvalError::scenario(format!(
                "scenario {} extractor failed for event {}: {source}",
                scenario.id,
                idx + 1
            ))
        })?;

        if scenario
            .persistence
            .as_ref()
            .is_some_and(|persistence| persistence.restart_after_event == idx + 1)
        {
            if let Some(path) = persistence_path {
                engine = persist_and_reload(&engine, config, path, &scenario.id)?;
            }
        }
    }

    if scenario.maintenance.compact_after_events {
        engine.compact();
        store_run.compacted = true;
    }

    let mut curation_actual = None;
    if scenario.maintenance.restore_from_backup {
        let Some(path) = persistence_path else {
            return Err(EvalError::scenario(format!(
                "scenario {} missing persistence path for restore",
                scenario.id
            )));
        };
        persist_to_store(&engine, path, &scenario.id)?;
    }

    if let Some(curation) = &scenario.maintenance.curation {
        let actual = apply_engine_curation(
            &mut engine,
            curation.apply,
            curation.compact,
            &mut store_run,
        )
        .map_err(|source| {
            EvalError::scenario(format!(
                "scenario {} failed to apply curation: {source}",
                scenario.id
            ))
        })?;
        curation_actual = Some(actual);
    }

    if scenario.maintenance.restore_from_backup {
        let Some(path) = persistence_path else {
            return Err(EvalError::scenario(format!(
                "scenario {} missing persistence path for restore",
                scenario.id
            )));
        };
        persist_to_store(&engine, path, &scenario.id)?;
        let store = JsonFileStore::new(path.to_path_buf());
        let restore = store.restore_from_backup().map_err(|source| {
            EvalError::scenario(format!(
                "scenario {} failed to restore store: {source}",
                scenario.id
            ))
        })?;
        if !restore.restored {
            return Err(EvalError::scenario(format!(
                "scenario {} restore did not restore from backup: {}",
                scenario.id, restore.action
            )));
        }
        store_run.restored = true;
        engine = MnemeEngine::from_store(config, &store).map_err(|source| {
            EvalError::scenario(format!(
                "scenario {} failed to reload restored store: {source}",
                scenario.id
            ))
        })?;
    }

    if scenario.maintenance.export_import_roundtrip {
        let Some(path) = persistence_path else {
            return Err(EvalError::scenario(format!(
                "scenario {} missing persistence path for export/import",
                scenario.id
            )));
        };
        let import_path = temp_store_path(&format!("{}-import", scenario.id));
        engine = export_import_roundtrip(&engine, config, path, &import_path, &scenario.id)?;
        let _ = std::fs::remove_file(&import_path);
        let _ = std::fs::remove_file(backup_path_for(&import_path));
        store_run.imported = true;
    }

    if scenario.maintenance.repair_from_backup {
        let Some(path) = persistence_path else {
            return Err(EvalError::scenario(format!(
                "scenario {} missing persistence path for repair",
                scenario.id
            )));
        };
        let store = JsonFileStore::new(path.to_path_buf());
        persist_to_store(&engine, path, &scenario.id)?;
        persist_to_store(&engine, path, &scenario.id)?;
        std::fs::write(path, "{not-json").map_err(|source| {
            EvalError::scenario(format!(
                "scenario {} failed to corrupt store for repair: {source}",
                scenario.id
            ))
        })?;
        let repair = store.repair_from_backup().map_err(|source| {
            EvalError::scenario(format!(
                "scenario {} failed to repair store: {source}",
                scenario.id
            ))
        })?;
        store_run.repair_performed = repair.repaired;
        engine = MnemeEngine::from_store(config, &store).map_err(|source| {
            EvalError::scenario(format!(
                "scenario {} failed to reload repaired store: {source}",
                scenario.id
            ))
        })?;
    }

    if let Some(agent_flow) = &scenario.agent_flow {
        let begin = engine.begin_session(SessionBeginInput {
            task: agent_flow.begin.task.clone(),
            actor_agent_id: agent_flow.begin.actor_agent_id.clone(),
            query: agent_flow.begin.query.clone(),
            allowed_scopes: effective_allowed_scopes(&agent_flow.begin.allowed_scopes),
            max_items: mneme_core::DEFAULT_CONTEXT_MAX_ITEMS,
        });
        if let Some(end) = &agent_flow.end {
            let input = SessionEndInput {
                session_id: begin.session.id,
                actor_agent_id: agent_flow.begin.actor_agent_id.clone(),
                summary: end.summary.clone(),
                remember: end.remember.clone(),
            };
            let result = match end.extractor {
                AgentEndExtractor::Rule => engine.end_session(input),
                AgentEndExtractor::Command => {
                    let Some(extractor) = command_extractor.as_ref() else {
                        return Err(EvalError::scenario(format!(
                            "scenario {} agent end command extractor requires mneme-v1-command target with --extractor-command",
                            scenario.id
                        )));
                    };
                    engine.end_session_with_extractor(
                        input,
                        extractor,
                        SessionMemoryInputMode::RawEvent,
                    )
                }
            };
            result.map(|_| ()).map_err(|source| {
                EvalError::scenario(format!(
                    "scenario {} failed to end agent session: {source}",
                    scenario.id
                ))
            })?;
        }
    }

    let context_pack = scenario.expected.context_pack.as_ref().map(|expected| {
        engine.build_context_pack_with(
            ContextQuery::with_allowed_scopes(
                expected.query.clone(),
                effective_allowed_scopes(&expected.allowed_scopes),
            )
            .with_max_items(
                expected
                    .max_items
                    .unwrap_or(mneme_core::DEFAULT_CONTEXT_MAX_ITEMS),
            ),
        )
    });
    if needs_store(scenario) {
        if let Some(path) = persistence_path {
            persist_to_store(&engine, path, &scenario.id)?;
        }
    }
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
        sessions: snapshot
            .sessions
            .into_iter()
            .map(|session| SessionActual {
                id: session.id,
                task: session.task,
                actor_agent_id: session.actor_agent_id,
                status: session.status.as_str().to_owned(),
                context_claim_ids: session.context_claim_ids,
                summary: session.summary,
                memory_event_ids: session.memory_event_ids,
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
                    score: item.score,
                    matched_terms: item.matched_terms,
                    match_reason: item.match_reason,
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
        store: persistence_path.map(|path| store_actual(path, &store_run)),
        quality: None,
        curation: curation_actual,
    };
    apply_seeded_fault(&mut actual, options.fault_mode);
    if scenario.expected.quality.is_some() {
        actual.quality = Some(build_quality_actual(&actual.claims));
    }
    Ok(actual)
}

fn persist_and_reload(
    engine: &MnemeEngine,
    config: MnemeConfig,
    path: &Path,
    scenario_id: &str,
) -> Result<MnemeEngine, EvalError> {
    persist_to_store(engine, path, scenario_id)?;
    let store = JsonFileStore::new(path.to_path_buf());
    MnemeEngine::from_store(config, &store).map_err(|source| {
        EvalError::scenario(format!(
            "scenario {scenario_id} failed to reload mneme-v1 state: {source}"
        ))
    })
}

fn persist_to_store(engine: &MnemeEngine, path: &Path, scenario_id: &str) -> Result<(), EvalError> {
    let mut store = JsonFileStore::new(path.to_path_buf());
    engine.persist(&mut store).map_err(|source| {
        EvalError::scenario(format!(
            "scenario {scenario_id} failed to persist mneme-v1 state: {source}"
        ))
    })
}

fn export_import_roundtrip(
    engine: &MnemeEngine,
    config: MnemeConfig,
    source_path: &Path,
    import_path: &Path,
    scenario_id: &str,
) -> Result<MnemeEngine, EvalError> {
    persist_to_store(engine, source_path, scenario_id)?;
    let source_store = JsonFileStore::new(source_path.to_path_buf());
    let state = source_store.load().map_err(|source| {
        EvalError::scenario(format!(
            "scenario {scenario_id} failed to export state: {source}"
        ))
    })?;
    let state = state
        .ok_or_else(|| EvalError::scenario(format!("scenario {scenario_id} exported no state")))?;
    let mut import_store = JsonFileStore::new(import_path.to_path_buf());
    import_store.save(&state).map_err(|source| {
        EvalError::scenario(format!(
            "scenario {scenario_id} failed to import state: {source}"
        ))
    })?;
    MnemeEngine::from_store(config, &import_store).map_err(|source| {
        EvalError::scenario(format!(
            "scenario {scenario_id} failed to reload imported state: {source}"
        ))
    })
}

fn apply_engine_curation(
    engine: &mut MnemeEngine,
    apply: bool,
    compact: bool,
    store_run: &mut StoreRunState,
) -> Result<crate::target::CurationActual, String> {
    let before_claims = target_claims_from_engine(engine);
    let plan = build_curation_plan_actual(&before_claims);
    let mut changed = false;
    let mut compacted = false;

    if apply {
        for claim_id in &plan.duplicate_forget_ids {
            engine
                .ingest_event(EventInput {
                    speaker_id: "system".to_owned(),
                    actor_agent_id: Some("mneme-curate".to_owned()),
                    text: format!("forget-id: {claim_id}"),
                    scope: "private".to_owned(),
                    trust_level: "system".to_owned(),
                })
                .map_err(|source| source.to_string())?;
            changed = true;
        }
        if compact && plan.compact_recommended {
            let compaction = engine.compact();
            compacted = compaction.removed_claims > 0 || compaction.removed_events > 0;
            store_run.compacted |= compacted;
            changed = true;
        }
    }

    let after_claims = target_claims_from_engine(engine);
    Ok(build_curation_actual(
        &before_claims,
        &after_claims,
        compacted,
        changed,
    ))
}

fn target_claims_from_engine(engine: &MnemeEngine) -> Vec<Claim> {
    engine
        .snapshot()
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
        .collect()
}

fn store_actual(path: &Path, run: &StoreRunState) -> StoreActual {
    let store = JsonFileStore::new(path.to_path_buf());
    let inspection = store.inspect();
    let validation = inspection.current.validation.as_ref();
    StoreActual {
        schema_version: inspection.current.schema_version,
        valid: inspection.current.status == StoreFileStatus::Valid,
        backup_present: inspection.backup.status != StoreFileStatus::Missing,
        repair_performed: run.repair_performed,
        restored: run.restored,
        compacted: run.compacted,
        imported: run.imported,
        generation: inspection.current.generation,
        error_count: validation.map_or(1, |report| report.error_count),
    }
}

#[derive(Debug, Default)]
struct StoreRunState {
    compacted: bool,
    imported: bool,
    repair_performed: bool,
    restored: bool,
}

fn needs_store(scenario: &Scenario) -> bool {
    scenario.persistence.is_some()
        || scenario.maintenance.export_import_roundtrip
        || scenario.maintenance.compact_after_events
        || scenario.maintenance.repair_from_backup
        || scenario.maintenance.curation.is_some()
        || scenario.maintenance.restore_from_backup
        || scenario.expected.store.is_some()
}

fn effective_allowed_scopes(scopes: &[String]) -> Vec<String> {
    if scopes.is_empty() {
        vec!["private".to_owned()]
    } else {
        scopes.iter().map(|scope| scope.trim().to_owned()).collect()
    }
}

fn backup_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("mneme-v1.json");
    path.with_file_name(format!("{file_name}.bak"))
}

fn temp_store_path(scenario_id: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "mneme-v1-eval-{}-{}-{unique}.json",
        std::process::id(),
        sanitize_file_component(scenario_id)
    ))
}

fn sanitize_file_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
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
