//! Read NDJSON {"stream":"inventory|control|actionables|journal:<id>","frame":{...}}.
//! Emit final projection JSON; invalid frames fail without fabricating a ready state.
use std::io::{self, BufRead};
use tekes_agent_contract::{Result, string};
use tekes_agent_session::MemoryProjectionStore;
fn main() -> Result<()> {
    let mut store = MemoryProjectionStore::new("replay");
    for line in io::stdin().lock().lines() {
        let line =
            line.map_err(|_| tekes_agent_contract::Error::Transport("stdin unavailable".into()))?;
        let value: serde_json::Value = serde_json::from_str(&line)?;
        store.apply(string(&value, "stream")?, value["frame"].clone())?;
    }
    println!("{}", serde_json::to_string_pretty(&store)?);
    Ok(())
}
