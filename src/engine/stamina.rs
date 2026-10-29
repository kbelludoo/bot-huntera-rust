use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

pub struct StaminaConfig {
    pub max_fatigue_threshold: f64,
    pub resume_fatigue_threshold: f64,
    pub min_rest_time: Duration,
}

impl Default for StaminaConfig {
    fn default() -> Self {
        Self {
            max_fatigue_threshold: 80.0,
            resume_fatigue_threshold: 30.0,
            min_rest_time: Duration::from_secs(300),
        }
    }
}

pub struct StaminaController {
    config: StaminaConfig,
    current_fatigue: RwLock<f64>,
    is_resting: RwLock<bool>,
    rest_started_at: RwLock<Option<Instant>>,
}

impl StaminaController {
    pub fn new(config: StaminaConfig) -> Self {
        Self {
            config,
            current_fatigue: RwLock::new(0.0),
            is_resting: RwLock::new(false),
            rest_started_at: RwLock::new(None),
        }
    }

    pub async fn should_hunt(&self) -> bool {
        let fatigue = *self.current_fatigue.read().await;
        let mut is_resting = self.is_resting.write().await;

        if *is_resting {
            let started = self.rest_started_at.read().await;
            if let Some(t) = *started {
                if t.elapsed() >= self.config.min_rest_time && fatigue <= self.config.resume_fatigue_threshold {
                    *is_resting = false;
                    return true;
                }
            }
            return false;
        }

        if fatigue >= self.config.max_fatigue_threshold {
            *is_resting = true;
            *self.rest_started_at.write().await = Some(Instant::now());
            return false;
        }

        true
    }

    pub async fn update_fatigue(&self, val: f64) {
        *self.current_fatigue.write().await = val;
    }

    pub async fn current_fatigue(&self) -> f64 {
        *self.current_fatigue.read().await
    }
}
