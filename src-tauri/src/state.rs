use crate::defaults::ManagedPolicy;
use crate::editor::EditManager;
use crate::logs::LogBus;
use crate::session::SessionManager;
use crate::store::Store;
use crate::transfers::TransferManager;
use std::sync::Arc;
pub struct AppState {
    pub store: Arc<Store>,
    pub sessions: Arc<SessionManager>,
    pub transfers: Arc<TransferManager>,
    pub logs: Arc<LogBus>,
    pub policy: Arc<ManagedPolicy>,
    pub edits: Arc<EditManager>,
}
