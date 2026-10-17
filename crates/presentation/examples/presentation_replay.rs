//! A second public-API consumer: project captured headless authority without a renderer or harness.
use serde::Deserialize;
use std::io::Read;
use tekes_agent_contract::{PromptMode, Result};
use tekes_agent_presentation::{ComposerDestination, ComposerDraft, PresentationStore, RunScope};
use tekes_agent_session::MemoryProjectionStore;
#[derive(Deserialize)]
struct Input {
    scope: RunScope,
    authority: MemoryProjectionStore,
    draft: Option<ComposerDraft>,
}
fn main() -> Result<()> {
    let mut text = String::new();
    std::io::stdin()
        .take(16 * 1024 * 1024)
        .read_to_string(&mut text)
        .map_err(|e| tekes_agent_contract::Error::Transport(e.to_string()))?;
    let input: Input = serde_json::from_str(&text)?;
    let mut store = PresentationStore::default();
    let destination = ComposerDestination::run(input.scope);
    store.select(destination.clone())?;
    if let Some(draft) = input.draft {
        store.edit(&destination, draft)?;
        let _prepared = store.prepare(&destination, PromptMode::Queue, &input.authority)?;
    }
    println!(
        "{}",
        serde_json::to_string(&store.refresh(&input.authority)?)?
    );
    Ok(())
}
