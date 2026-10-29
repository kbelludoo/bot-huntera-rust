use crate::game::bot::BotInstance;
use crate::game::types::AccountConfig;
use std::sync::Arc;

pub struct Supervisor {
    bots: Vec<Arc<BotInstance>>,
}

impl Supervisor {
    pub fn new(configs: Vec<AccountConfig>) -> Self {
        let mut bots = Vec::new();
        for cfg in configs {
            bots.push(Arc::new(BotInstance::new(cfg)));
        }
        Self { bots }
    }

    pub fn bots(&self) -> &[Arc<BotInstance>] {
        &self.bots
    }

    pub async fn start_all(&self) {
        for bot in &self.bots {
            let b = Arc::clone(bot);
            tokio::spawn(async move {
                b.run_loop().await;
            });
        }
    }
}
