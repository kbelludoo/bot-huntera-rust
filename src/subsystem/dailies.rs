use chrono::{Datelike, Utc};
use std::sync::atomic::{AtomicU32, Ordering};

pub struct DailyManager {
    last_claim_day: AtomicU32,
}

impl Default for DailyManager {
    fn default() -> Self {
        Self {
            last_claim_day: AtomicU32::new(0),
        }
    }
}

impl DailyManager {
    pub fn needs_claim(&self) -> bool {
        let today = Utc::now().day();
        self.last_claim_day.load(Ordering::Relaxed) != today
    }

    pub fn record_claim(&self) {
        let today = Utc::now().day();
        self.last_claim_day.store(today, Ordering::Relaxed);
    }
}
