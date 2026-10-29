//! Controle de stamina/fadiga do Huntera.
//!
//! O jogo usa **stamina decrescente** em milissegundos (≈42h cheias). Abaixo de
//! 14h o ganho de EXP/drops entra em penalidade, então o bot recua para treino
//! e só volta com ≥20h (histerese anti-thrashing) e após o descanso mínimo.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use tokio::time::{Duration, Instant};

/// Stamina cheia de referência (42h).
pub const FULL_STAMINA_MS: i64 = 42 * 3_600_000;
/// Limiar de parada (14h) — abaixo disso o farm é penalizado.
pub const MIN_HUNT_STAMINA_MS: i64 = 14 * 3_600_000;
/// Limiar de retomada (20h) — evita liga/desliga em ciclos curtos.
pub const RESUME_STAMINA_MS: i64 = 20 * 3_600_000;

pub struct StaminaConfig {
    pub max_fatigue_threshold: f64,
    pub resume_fatigue_threshold: f64,
    pub min_rest_time: Duration,
    pub min_stamina_ms: i64,
    pub resume_stamina_ms: i64,
}

impl Default for StaminaConfig {
    fn default() -> Self {
        Self {
            max_fatigue_threshold: 80.0,
            resume_fatigue_threshold: 30.0,
            min_rest_time: Duration::from_secs(300),
            min_stamina_ms: MIN_HUNT_STAMINA_MS,
            resume_stamina_ms: RESUME_STAMINA_MS,
        }
    }
}

pub struct StaminaController {
    config: StaminaConfig,
    stamina_ms: AtomicI64,
    is_resting: AtomicBool,
    rest_started_at: std::sync::Mutex<Option<Instant>>,
}

impl StaminaController {
    pub fn new(config: StaminaConfig) -> Self {
        Self {
            config,
            stamina_ms: AtomicI64::new(-1),
            is_resting: AtomicBool::new(false),
            rest_started_at: std::sync::Mutex::new(None),
        }
    }

    /// `true` enquanto o bot pode caçar (respeitando histerese de descanso).
    pub async fn should_hunt(&self) -> bool {
        let resting = self.is_resting.load(Ordering::SeqCst);
        let can_resume = self.met_resume_threshold();

        if resting {
            let elapsed = self
                .rest_started_at
                .lock()
                .ok()
                .and_then(|g| *g)
                .map(|t| t.elapsed())
                .unwrap_or_default();

            if elapsed >= self.config.min_rest_time && can_resume {
                self.is_resting.store(false, Ordering::SeqCst);
                return true;
            }
            return false;
        }

        if self.needs_rest() {
            self.is_resting.store(true, Ordering::SeqCst);
            if let Ok(mut g) = self.rest_started_at.lock() {
                *g = Some(Instant::now());
            }
            return false;
        }

        true
    }

    fn needs_rest(&self) -> bool {
        let ms = self.stamina_ms.load(Ordering::Relaxed);
        if ms >= 0 {
            return ms <= self.config.min_stamina_ms;
        }
        self.current_fatigue_sync() >= self.config.max_fatigue_threshold
    }

    fn met_resume_threshold(&self) -> bool {
        let ms = self.stamina_ms.load(Ordering::Relaxed);
        if ms >= 0 {
            return ms >= self.config.resume_stamina_ms;
        }
        self.current_fatigue_sync() <= self.config.resume_fatigue_threshold
    }

    pub fn update_stamina_ms(&self, ms: i64) {
        if ms >= 0 {
            self.stamina_ms.store(ms, Ordering::Relaxed);
        }
    }

    pub fn stamina_ms(&self) -> Option<i64> {
        let ms = self.stamina_ms.load(Ordering::Relaxed);
        if ms >= 0 {
            Some(ms)
        } else {
            None
        }
    }

    /// Fadiga derivada da stamina (0–100%).
    pub fn current_fatigue_sync(&self) -> f64 {
        match self.stamina_ms() {
            Some(ms) => {
                let used = FULL_STAMINA_MS.saturating_sub(ms).max(0) as f64;
                ((used / FULL_STAMINA_MS as f64) * 100.0 * 10.0).round() / 10.0
            }
            None => 0.0,
        }
    }

    pub async fn current_fatigue(&self) -> f64 {
        self.current_fatigue_sync()
    }

    pub fn is_resting(&self) -> bool {
        self.is_resting.load(Ordering::SeqCst)
    }

    pub fn force_rest(&self) {
        if !self.is_resting.swap(true, Ordering::SeqCst) {
            if let Ok(mut g) = self.rest_started_at.lock() {
                *g = Some(Instant::now());
            }
        }
    }

    pub fn force_resume(&self) {
        self.is_resting.store(false, Ordering::SeqCst);
    }

    /// Horas de stamina restantes (para telemetria).
    pub fn stamina_hours(&self) -> f64 {
        self.stamina_ms()
            .map(|ms| ((ms as f64 / 3_600_000.0) * 10.0).round() / 10.0)
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rests_below_14h_and_resumes_only_above_20h() {
        let c = StaminaController::new(StaminaConfig {
            min_rest_time: Duration::from_millis(0),
            ..Default::default()
        });

        c.update_stamina_ms(MIN_HUNT_STAMINA_MS - 1);
        assert!(!c.should_hunt().await);
        assert!(c.is_resting());

        c.update_stamina_ms(15 * 3_600_000);
        assert!(!c.should_hunt().await, "15h ainda não é limiar de retomada");

        c.update_stamina_ms(RESUME_STAMINA_MS + 1);
        assert!(c.should_hunt().await);
        assert!(!c.is_resting());
    }

    #[tokio::test]
    async fn hunts_with_healthy_stamina() {
        let c = StaminaController::new(StaminaConfig::default());
        c.update_stamina_ms(40 * 3_600_000);
        assert!(c.should_hunt().await);
        assert!(c.stamina_hours() > 39.0);
        assert!(c.current_fatigue().await < 10.0);
    }
}
