//! Notification-only host adapter over the existing presentation subscription.
//! Returning false stops forwarding after the native target has closed.
pub async fn forward<T: Send + Sync + 'static>(
    mut changes: tokio::sync::watch::Receiver<T>,
    mut notify: impl FnMut() -> bool + Send,
) {
    // Subscribe before the initial UI snapshot; publish one wake to cover startup.
    if !notify() {
        return;
    }
    while changes.changed().await.is_ok() {
        // Consume the latest revision BEFORE notifying; an update during notification
        // remains unseen and causes another iteration, rather than losing the tail.
        drop(changes.borrow_and_update());
        if !notify() {
            break;
        }
    }
}
