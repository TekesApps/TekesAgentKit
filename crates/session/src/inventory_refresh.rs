//! Fresh public cursors; the runtime actor alone commits or retires readers.
use futures_util::StreamExt;
use std::{sync::Arc, time::Duration};
use tekes_agent_contract::*;

pub(crate) struct InventoryCursor {
    pub baseline: serde_json::Value,
    pub frames: FrameStream,
}
pub(crate) struct InventoryRefresh {
    pub inventory: InventoryCursor,
    pub workspace: Option<InventoryCursor>,
}
async fn baseline(mut frames: FrameStream) -> Result<InventoryCursor> {
    let baseline = frames.next().await.ok_or(Error::Disconnected)??;
    require(
        baseline["type"] == "baseline",
        "inventory reader did not begin with baseline",
    )?;
    Ok(InventoryCursor { baseline, frames })
}
pub(crate) async fn read(endpoint: Arc<dyn SessionEndpoint>) -> Result<InventoryRefresh> {
    tokio::time::timeout(Duration::from_secs(30), async {
        // Establish both subscriptions before reading either baseline so the
        // cursors buffer subsequent deltas while the atomic pair is prepared.
        let workspace = if endpoint.host().capabilities.contains("workspace-sync") {
            Some(endpoint.follow(StreamTarget::WorkspaceInventory).await?)
        } else {
            None
        };
        let inventory = endpoint.follow(StreamTarget::SessionInventory).await?;
        let workspace = match workspace {
            Some(frames) => Some(baseline(frames).await?),
            None => None,
        };
        Ok(InventoryRefresh {
            inventory: baseline(inventory).await?,
            workspace,
        })
    })
    .await
    .map_err(|_| Error::Timeout)?
}
