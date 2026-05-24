//! Minimal personal-memory flow using the `mneme-core` API.

use mneme_core::{EventInput, InMemoryStore, MnemeConfig, MnemeEngine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = InMemoryStore::new();
    let mut engine = MnemeEngine::new(MnemeConfig::default());

    engine.ingest_event(EventInput {
        speaker_id: "user".to_owned(),
        actor_agent_id: Some("codex".to_owned()),
        text: "remember: user prefers local-first tools".to_owned(),
        scope: "private".to_owned(),
        trust_level: "explicit".to_owned(),
    })?;
    engine.persist(&mut store)?;

    let mut reloaded = MnemeEngine::from_store(MnemeConfig::default(), &store)?;
    let context = reloaded.build_context_pack("local-first");

    assert_eq!(context.items.len(), 1);
    assert_eq!(
        context.items[0].claim_text,
        "user prefers local-first tools"
    );
    Ok(())
}
