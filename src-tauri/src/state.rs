use crate::auth::token::TokenManager;
use std::sync::Arc;

pub struct AppState {
    pub token_manager: Arc<TokenManager>,
}

impl AppState {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let token_path = data_dir.join("proxy-tokens.json");
        Self {
            token_manager: Arc::new(TokenManager::new(token_path)),
        }
    }
}
