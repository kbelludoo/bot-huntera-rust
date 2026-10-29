use std::sync::atomic::{AtomicU64, Ordering};

pub struct AdaptiveSpeedTuner {
    target_rate_ms: AtomicU64,
}

impl Default for AdaptiveSpeedTuner {
    fn default() -> Self {
        Self {
            target_rate_ms: AtomicU64::new(1500),
        }
    }
}

impl AdaptiveSpeedTuner {
    pub fn target_rate_ms(&self) -> u64 {
        self.target_rate_ms.load(Ordering::Relaxed)
    }

    pub fn on_429(&self) {
        self.target_rate_ms.fetch_add(500, Ordering::SeqCst);
    }

    pub fn on_success(&self) {
        let curr = self.target_rate_ms.load(Ordering::Relaxed);
        if curr > 1000 {
            self.target_rate_ms.fetch_sub(10, Ordering::Relaxed);
        }
    }
}
