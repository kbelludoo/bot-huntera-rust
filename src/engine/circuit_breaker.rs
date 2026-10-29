use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub max_failures: u32,
    pub cooldown: Duration,
    pub half_open_success_needed: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            max_failures: 3,
            cooldown: Duration::from_secs(300),
            half_open_success_needed: 2,
        }
    }
}

pub struct CircuitBreaker {
    pub id: String,
    state: RwLock<CircuitState>,
    failures: AtomicU32,
    half_open_successes: AtomicU32,
    last_trip: RwLock<Option<Instant>>,
    config: CircuitBreakerConfig,
    trip_count: AtomicU64,
}

impl CircuitBreaker {
    pub fn new(id: &str, config: CircuitBreakerConfig) -> Self {
        Self {
            id: id.to_string(),
            state: RwLock::new(CircuitState::Closed),
            failures: AtomicU32::new(0),
            half_open_successes: AtomicU32::new(0),
            last_trip: RwLock::new(None),
            config,
            trip_count: AtomicU64::new(0),
        }
    }

    pub async fn can_execute(&self) -> bool {
        let mut state = self.state.write().await;
        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                let last = self.last_trip.read().await;
                if let Some(t) = *last {
                    if t.elapsed() >= self.config.cooldown {
                        *state = CircuitState::HalfOpen;
                        self.half_open_successes.store(0, Ordering::SeqCst);
                        warn!("[CB: {}] Transição OPEN -> HALF-OPEN para teste de recuperação.", self.id);
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    pub async fn record_success(&self) {
        let mut state = self.state.write().await;
        match *state {
            CircuitState::HalfOpen => {
                let n = self.half_open_successes.fetch_add(1, Ordering::SeqCst) + 1;
                if n >= self.config.half_open_success_needed {
                    *state = CircuitState::Closed;
                    self.failures.store(0, Ordering::SeqCst);
                    info!("[CB: {}] Recuperação completa! Circuito normalizado (CLOSED).", self.id);
                }
            }
            CircuitState::Closed => {
                self.failures.store(0, Ordering::SeqCst);
            }
            CircuitState::Open => {}
        }
    }

    pub async fn record_failure(&self, reason: &str) {
        let failures = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        warn!("[CB: {}] Falha registrada ({}/{}): {}", self.id, failures, self.config.max_failures, reason);

        if failures >= self.config.max_failures {
            let mut state = self.state.write().await;
            if *state != CircuitState::Open {
                *state = CircuitState::Open;
                *self.last_trip.write().await = Some(Instant::now());
                self.trip_count.fetch_add(1, Ordering::Relaxed);
                error!("🚨 [CB: {}] DISJUNTOR ANTI-BAN DESARMADO! Pausando ações por {:?}.", self.id, self.config.cooldown);
            }
        }
    }

    pub async fn state(&self) -> CircuitState {
        *self.state.read().await
    }
}
