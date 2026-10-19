//! A second public consumer uses AgentKit persistence without the Web service.
use tekes_agent_contract::{ThreadActionHost, ThreadActionRequest};
use tekes_agent_presentation::ThreadPreferences;
#[tokio::main]
async fn main() -> tekes_agent_contract::Result<()> {
    let root = std::env::temp_dir().join(format!("agentkit-pin-consumer-{}", std::process::id()));
    let path = root.join("preferences.json");
    let request = ThreadActionRequest {
        session_id: "example".into(),
        action_id: "pin".into(),
        title: None,
    };
    {
        let preferences = ThreadPreferences::open(&path, "example/workspace")?;
        preferences
            .perform("example", &request, "consumer-pin")
            .await?;
    }
    {
        let reopened = ThreadPreferences::open(&path, "example/workspace")?;
        assert!(reopened.pin_position("example", "example").unwrap().pinned);
        assert!(reopened.replay("example", &request, "consumer-pin")?);
    }
    std::fs::remove_dir_all(root).unwrap();
    println!("THREAD_PREFERENCES_PUBLIC_CONSUMER_OK");
    Ok(())
}
