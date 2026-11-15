use crate::auth::session::RefreshLocks;
use crate::auth::token::TokenManager;
use std::sync::Arc;

pub struct AppState {
    pub token_manager: Arc<TokenManager>,
    /// 按账号串行化凭证刷新，防止并发轮换互相覆盖
    pub refresh_locks: Arc<RefreshLocks>,
}

impl AppState {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let token_path = data_dir.join("proxy-tokens.json");
        Self {
            token_manager: Arc::new(TokenManager::new(token_path)),
            refresh_locks: Arc::new(RefreshLocks::new()),
        }
    }
}
