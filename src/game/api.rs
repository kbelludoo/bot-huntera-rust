use crate::game::types::AccountConfig;
use crate::network::client::StealthHttpClient;
use std::sync::Arc;

pub struct GameApiClient {
    http: Arc<StealthHttpClient>,
    config: AccountConfig,
    base_url: String,
}

impl GameApiClient {
    pub fn new(config: AccountConfig, http: Arc<StealthHttpClient>) -> Self {
        let base = if config.credentials.origin.is_empty() {
            "https://huntera.net".to_string()
        } else {
            config.credentials.origin.clone()
        };
        Self {
            http,
            config,
            base_url: base,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
