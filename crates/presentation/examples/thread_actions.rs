//! A second public-API consumer: a Rust renderer needs no Web menu policy.
use std::collections::BTreeSet;
use tekes_agent_contract::EndpointCapabilities;
use tekes_agent_presentation::{ThreadActionContext, thread_actions};
fn main() {
    let capabilities = EndpointCapabilities::methods(&["session.rename", "session.fork"]);
    let definition = thread_actions(&ThreadActionContext {
        endpoint: "example",
        session_id: "thread",
        kind: "pi",
        url_scheme: "agent",
        capabilities: &capabilities,
        ready: true,
        running: false,
        archived: false,
        pin: None,
        host_methods: &BTreeSet::new(),
    });
    assert_eq!(definition.items[1].id, "rename");
    assert_eq!(
        definition.items[3].copy_text.as_deref(),
        Some("agent://pi-stdio-rpc/thread")
    );
    println!("{}", serde_json::to_string(&definition).unwrap());
}
