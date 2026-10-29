use crate::engine::party::TeamState;
use crate::game::bot::BotInstance;
use crate::game::types::AccountConfig;
use std::sync::Arc;
use tracing::info;

pub struct Supervisor {
    bots: Vec<Arc<BotInstance>>,
    team: Arc<TeamState>,
}

impl Supervisor {
    pub fn new(configs: Vec<AccountConfig>) -> Self {
        // O recuo de stamina é coletivo: todos os membros precisam recuperar.
        let team = Arc::new(TeamState::new(configs.len()));
        let bots = configs
            .into_iter()
            .map(|cfg| {
                let id = cfg.id.clone();
                let party = cfg.party_config();
                if matches!(party.role, crate::engine::party::Role::Leader) && party.members.is_empty()
                {
                    info!("[Supervisor] '{id}' é líder mas não tem partyMembers configurados.");
                }
                Arc::new(BotInstance::new(cfg, Arc::clone(&team)))
            })
            .collect();

        Self { bots, team }
    }

    pub fn bots(&self) -> &[Arc<BotInstance>] {
        &self.bots
    }

    pub fn team(&self) -> &Arc<TeamState> {
        &self.team
    }

    pub async fn start_all(&self) {
        for bot in &self.bots {
            let b = Arc::clone(bot);
            tokio::spawn(async move {
                b.run_loop().await;
            });
        }
    }

    pub async fn stop_all(&self) {
        use std::sync::atomic::Ordering;
        for bot in &self.bots {
            bot.running.store(false, Ordering::SeqCst);
        }
    }
}
