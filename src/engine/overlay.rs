use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct StateOverlay {
    raw_state: RwLock<HashMap<String, Value>>,
}

impl Default for StateOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl StateOverlay {
    pub fn new() -> Self {
        Self {
            raw_state: RwLock::new(HashMap::new()),
        }
    }

    pub async fn merge_unknown_fields(&self, incoming: HashMap<String, Value>) {
        let mut current = self.raw_state.write().await;
        for (k, v) in incoming {
            current.insert(k, v);
        }
    }

    pub async fn get_raw_state(&self) -> HashMap<String, Value> {
        self.raw_state.read().await.clone()
    }
}
