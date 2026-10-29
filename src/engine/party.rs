//! Coordenação de Party / Equipe (4 vocações) e taxa coletiva de abates.
//!
//! Regras aplicadas:
//! - **Taxa coletiva**: `KillRate_individual <= solo / (N * (1 + bônus))`.
//!   Quanto maior a party, MENOR a taxa de cada membro (anti-detecção).
//! - **Sincronia de stamina**: se uma conta fica sem stamina, TODO o time recua
//!   para treino; só volta a caçar quando todos recuperaram (histerese).
//! - **Liderança**: apenas o líder inicia caçada; seguidores entram na sala via
//!   `party-follow-leader` / `hunt-team-respond`.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tokio::time::Duration;

/// Bônus padrão de EXP com 4 vocações distintas na party.
pub const DEFAULT_PARTY_BONUS_PERCENT: f64 = 45.0;
/// Teto seguro de abates/minuto caçando sozinho (base do cálculo coletivo).
pub const SAFE_SOLO_KILL_RATE_PER_MIN: f64 = 40.0;
/// Intervalo mínimo entre ações de combate (nunca mais rápido que isso).
const MIN_ACTION_INTERVAL: Duration = Duration::from_millis(1200);
/// Intervalo máximo (party grande o suficiente para quase parar a caça).
const MAX_ACTION_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[default]
    Solo,
    Leader,
    Follower,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Solo => "solo",
            Role::Leader => "leader",
            Role::Follower => "follower",
        }
    }

    pub fn is_leader(self) -> bool {
        matches!(self, Role::Leader)
    }

    pub fn is_follower(self) -> bool {
        matches!(self, Role::Follower)
    }

    pub fn is_solo(self) -> bool {
        matches!(self, Role::Solo)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyConfig {
    #[serde(default)]
    pub role: Role,
    /// Nomes dos OUTROS personagens da party (sem incluir a si mesmo).
    #[serde(default, alias = "partyMembers")]
    pub members: Vec<String>,
    #[serde(default = "default_bonus", alias = "partyBonusPercent")]
    pub bonus_percent: f64,
    #[serde(default = "default_solo_rate", alias = "soloKillRate")]
    pub solo_kill_rate_per_min: f64,
}

fn default_bonus() -> f64 {
    DEFAULT_PARTY_BONUS_PERCENT
}

fn default_solo_rate() -> f64 {
    SAFE_SOLO_KILL_RATE_PER_MIN
}

impl Default for PartyConfig {
    fn default() -> Self {
        Self {
            role: Role::Solo,
            members: Vec::new(),
            bonus_percent: DEFAULT_PARTY_BONUS_PERCENT,
            solo_kill_rate_per_min: SAFE_SOLO_KILL_RATE_PER_MIN,
        }
    }
}

/// `KillRate_individual = solo / (N * (1 + bônus))`.
pub fn calculate_party_max_kill_rate(solo_rate: f64, party_size: usize, bonus_percent: f64) -> f64 {
    let members = party_size.max(1) as f64;
    let bonus = bonus_percent.max(0.0) / 100.0;
    let rate = solo_rate.max(0.0) / (members * (1.0 + bonus));
    if rate.is_finite() {
        rate
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PartySnapshot {
    pub role: &'static str,
    pub in_party: bool,
    pub party_size: usize,
    pub leader: Option<String>,
    pub bonus_percent: f64,
    pub kill_rate_per_min: f64,
    pub invites_sent: u64,
    pub follows_sent: u64,
    pub costs_accepted: bool,
}

pub struct PartyCoordinator {
    config: PartyConfig,
    in_party: AtomicBool,
    party_size: AtomicUsize,
    leader: Mutex<Option<String>>,
    invites_sent: AtomicU64,
    follows_sent: AtomicU64,
    costs_accepted: AtomicBool,
    last_invite_at: Mutex<Option<Instant>>,
}

impl PartyCoordinator {
    pub fn new(config: PartyConfig) -> Self {
        let declared = config.members.len() + 1;
        Self {
            config,
            in_party: AtomicBool::new(false),
            party_size: AtomicUsize::new(declared),
            leader: Mutex::new(None),
            invites_sent: AtomicU64::new(0),
            follows_sent: AtomicU64::new(0),
            costs_accepted: AtomicBool::new(false),
            last_invite_at: Mutex::new(None),
        }
    }

    pub fn role(&self) -> Role {
        self.config.role
    }

    pub fn config(&self) -> &PartyConfig {
        &self.config
    }

    pub fn members(&self) -> &[String] {
        &self.config.members
    }

    pub fn is_in_party(&self) -> bool {
        self.in_party.load(Ordering::Relaxed)
    }

    pub fn party_size(&self) -> usize {
        if self.is_in_party() {
            self.party_size.load(Ordering::Relaxed).max(1)
        } else {
            1
        }
    }

    pub fn leader(&self) -> Option<String> {
        self.leader.lock().ok().and_then(|g| g.clone())
    }

    /// Abates/minuto permitidos para ESTE membro agora.
    pub fn kill_rate_per_min(&self) -> f64 {
        calculate_party_max_kill_rate(
            self.config.solo_kill_rate_per_min,
            self.party_size(),
            self.config.bonus_percent,
        )
    }

    /// Intervalo entre ações de combate respeitando a taxa coletiva + jitter.
    pub fn action_interval(&self) -> Duration {
        let rate = self.kill_rate_per_min();
        if rate <= 0.0 {
            return MAX_ACTION_INTERVAL;
        }
        let base = 60.0 / rate;
        // ±10% de jitter para não gerar cadência robótica.
        let jitter = 0.9 + rand::random::<f64>() * 0.2;
        let secs = (base * jitter).clamp(
            MIN_ACTION_INTERVAL.as_secs_f64(),
            MAX_ACTION_INTERVAL.as_secs_f64(),
        );
        Duration::from_secs_f64(secs)
    }

    pub fn should_invite_now(&self) -> bool {
        if !self.config.role.is_leader() || self.config.members.is_empty() {
            return false;
        }
        let mut last = match self.last_invite_at.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match *last {
            Some(t) if t.elapsed() < Duration::from_secs(12) => false,
            _ => {
                *last = Some(Instant::now());
                true
            }
        }
    }

    pub fn note_invite(&self) {
        self.invites_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn note_follow(&self) {
        self.follows_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// Atualiza o estado a partir do evento `party-update` do servidor.
    pub fn apply_party_update(&self, party_size: usize, leader: Option<String>) {
        if party_size > 1 {
            self.in_party.store(true, Ordering::Relaxed);
            self.party_size.store(party_size, Ordering::Relaxed);
        }
        if let Ok(mut g) = self.leader.lock() {
            *g = leader;
        }
    }

    pub fn set_in_party(&self, in_party: bool) {
        self.in_party.store(in_party, Ordering::Relaxed);
        if !in_party {
            self.party_size.store(1, Ordering::Relaxed);
        }
    }

    pub fn set_costs_accepted(&self, accepted: bool) {
        self.costs_accepted.store(accepted, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> PartySnapshot {
        PartySnapshot {
            role: self.config.role.as_str(),
            in_party: self.is_in_party(),
            party_size: self.party_size(),
            leader: self.leader(),
            bonus_percent: self.config.bonus_percent,
            kill_rate_per_min: (self.kill_rate_per_min() * 100.0).round() / 100.0,
            invites_sent: self.invites_sent.load(Ordering::Relaxed),
            follows_sent: self.follows_sent.load(Ordering::Relaxed),
            costs_accepted: self.costs_accepted.load(Ordering::Relaxed),
        }
    }
}

/// Estado compartilhado do time: garante que todos treinem juntos e voltem juntos.
pub struct TeamState {
    resting: AtomicBool,
    epoch: AtomicU64,
    expected: AtomicUsize,
    recovered: AtomicUsize,
    reason: Mutex<String>,
}

impl TeamState {
    pub fn new(team_size: usize) -> Self {
        Self {
            resting: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            expected: AtomicUsize::new(team_size.max(1)),
            recovered: AtomicUsize::new(0),
            reason: Mutex::new(String::new()),
        }
    }

    pub fn is_resting(&self) -> bool {
        self.resting.load(Ordering::SeqCst)
    }

    pub fn reason(&self) -> String {
        self.reason
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|p| p.into_inner().clone())
    }

    pub fn rest_epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub fn expected(&self) -> usize {
        self.expected.load(Ordering::SeqCst)
    }

    pub fn recovered(&self) -> usize {
        self.recovered.load(Ordering::SeqCst)
    }

    /// Inicia (ou reforça) o recuo coletivo. Retorna `true` se é um novo ciclo.
    pub fn begin_rest(&self, reason: impl Into<String>) -> bool {
        if let Ok(mut r) = self.reason.lock() {
            *r = reason.into();
        }
        if self.resting.swap(true, Ordering::SeqCst) {
            false
        } else {
            self.recovered.store(0, Ordering::SeqCst);
            self.epoch.fetch_add(1, Ordering::SeqCst);
            true
        }
    }

    /// Marca que um membro recuperou stamina. Retorna `true` quando todos voltaram.
    pub fn mark_recovered(&self, epoch: u64) -> bool {
        if epoch != self.rest_epoch() || !self.is_resting() {
            return false;
        }
        let done = self.recovered.fetch_add(1, Ordering::SeqCst) + 1;
        if done >= self.expected().max(1) {
            self.resting.store(false, Ordering::SeqCst);
            self.recovered.store(0, Ordering::SeqCst);
            return true;
        }
        false
    }

    pub fn force_resume(&self) {
        self.resting.store(false, Ordering::SeqCst);
        self.recovered.store(0, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collective_kill_rate_follows_rule() {
        // Exemplo normativo: solo 40/min, party 4, bônus 45% => ~6.89/min
        let rate = calculate_party_max_kill_rate(40.0, 4, 45.0);
        assert!((rate - 6.896).abs() < 0.01, "rate = {rate}");
    }

    #[test]
    fn solo_kill_rate_is_unchanged() {
        assert!((calculate_party_max_kill_rate(40.0, 1, 0.0) - 40.0).abs() < 1e-9);
    }

    #[test]
    fn party_interval_is_slower_than_solo() {
        let solo = PartyCoordinator::new(PartyConfig {
            role: Role::Solo,
            ..Default::default()
        });
        let party = PartyCoordinator::new(PartyConfig {
            role: Role::Leader,
            members: vec!["b".into(), "c".into(), "d".into()],
            ..Default::default()
        });
        party.set_in_party(true);

        assert!(party.action_interval() > solo.action_interval());
        assert!(party.action_interval() <= MAX_ACTION_INTERVAL);
    }

    #[test]
    fn leader_invite_is_throttled() {
        let leader = PartyCoordinator::new(PartyConfig {
            role: Role::Leader,
            members: vec!["b".into()],
            ..Default::default()
        });
        assert!(leader.should_invite_now());
        assert!(!leader.should_invite_now());
    }

    #[test]
    fn team_rest_waits_for_every_member() {
        let team = TeamState::new(2);
        assert!(team.begin_rest("stamina"));
        assert!(!team.begin_rest("stamina"));
        let epoch = team.rest_epoch();

        assert!(!team.mark_recovered(epoch));
        assert!(team.is_resting());
        assert!(team.mark_recovered(epoch));
        assert!(!team.is_resting());
    }

    #[test]
    fn stale_epoch_is_ignored() {
        let team = TeamState::new(1);
        team.begin_rest("x");
        let epoch = team.rest_epoch();
        team.force_resume();
        assert!(!team.mark_recovered(epoch));
    }
}
