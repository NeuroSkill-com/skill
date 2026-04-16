use tokio::sync::oneshot;

/// Handle returned to the caller so a session can be cancelled.
pub struct SessionHandle {
    pub cancel_tx: oneshot::Sender<()>,
}
