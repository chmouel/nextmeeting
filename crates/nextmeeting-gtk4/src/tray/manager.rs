use std::sync::{mpsc::Sender, Arc};

use ksni::TrayMethods;
use tokio::runtime::Runtime;
use tracing::warn;

use super::{sni::NextMeetingTray, TrayCommand};

#[derive(Debug)]
pub struct TrayManager {
    runtime: Arc<Runtime>,
    tx: Sender<TrayCommand>,
}

impl TrayManager {
    pub fn new(runtime: Arc<Runtime>, tx: Sender<TrayCommand>) -> Self {
        Self { runtime, tx }
    }

    pub fn start(&self) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let tray = NextMeetingTray::new(tx);
            match tray.spawn().await {
                Ok(handle) => {
                    let _keep_alive = handle;
                    std::future::pending::<()>().await;
                }
                Err(err) => {
                    warn!(error = %err, "failed to start tray backend");
                }
            }
        });
    }
}
