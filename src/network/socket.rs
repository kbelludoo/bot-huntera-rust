use std::time::Duration;

pub struct SocketClientConfig {
    pub url: String,
    pub ping_interval: Duration,
    pub reconnect_base: Duration,
}

impl Default for SocketClientConfig {
    fn default() -> Self {
        Self {
            url: "https://huntera.net/ws".to_string(),
            ping_interval: Duration::from_secs(25),
            reconnect_base: Duration::from_millis(500),
        }
    }
}

pub struct SocketClient {
    pub config: SocketClientConfig,
}

impl SocketClient {
    pub fn new(config: SocketClientConfig) -> Self {
        Self { config }
    }
}
