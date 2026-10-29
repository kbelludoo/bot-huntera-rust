use crate::engine::party::{PartyConfig, Role};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub credentials: GameCredentials,
    #[serde(default)]
    pub settings: GameSettings,
    /// `leader` inicia a party/hunt, `follower` apenas segue o líder.
    #[serde(default)]
    pub role: Role,
    /// Nomes dos outros personagens da party (sem incluir a si mesmo).
    #[serde(default, alias = "partyMembers")]
    pub party_members: Vec<String>,
    /// Bônus de EXP da party em % (padrão: 45% com 4 vocações).
    #[serde(default, alias = "partyBonusPercent")]
    pub party_bonus_percent: Option<f64>,
    /// Teto seguro de abates/minuto caçando sozinho (base do throttle coletivo).
    #[serde(default, alias = "soloKillRate")]
    pub solo_kill_rate: Option<f64>,
    /// Compatibilidade com configs antigas: parceiro único.
    #[serde(default, alias = "partnerName")]
    pub partner_name: String,
    /// Tier da caçada (padrão 0).
    #[serde(default)]
    pub tier: Option<u32>,
}

impl AccountConfig {
    pub fn party_config(&self) -> PartyConfig {
        let mut members = self.party_members.clone();
        if members.is_empty() && !self.partner_name.is_empty() {
            members.push(self.partner_name.clone());
        }
        members.retain(|m| !m.trim().is_empty());

        let defaults = PartyConfig::default();
        PartyConfig {
            role: self.role,
            members,
            bonus_percent: self.party_bonus_percent.unwrap_or(defaults.bonus_percent),
            solo_kill_rate_per_min: self
                .solo_kill_rate
                .unwrap_or(defaults.solo_kill_rate_per_min),
        }
    }

    pub fn tier(&self) -> u32 {
        self.tier.unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameCredentials {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub origin: String,
}

impl GameCredentials {
    /// `true` quando o token parece um cookie de sessão (`a=b; c=d`) e não um ticket.
    pub fn token_is_cookie(&self) -> bool {
        self.token.contains('=') && !self.token.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameSettings {
    #[serde(default)]
    pub hunt_target: String,
    #[serde(default)]
    pub auto_hunt: bool,
    #[serde(default)]
    pub auto_sell: bool,
    #[serde(default)]
    pub auto_heal: bool,
    #[serde(default)]
    pub auto_daily: bool,
    #[serde(default)]
    pub auto_treino: bool,
    #[serde(default)]
    pub auto_catch: bool,
    #[serde(default)]
    pub auto_bless: bool,
    #[serde(default)]
    pub auto_shadows: bool,
    #[serde(default)]
    pub auto_transmute: bool,
    #[serde(default)]
    pub focus_bestiary: bool,
    #[serde(default)]
    pub exploit_weakness: bool,
    #[serde(default)]
    pub vip_farmer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketHeader {
    pub opcode: u16,
    pub length: u32,
    pub sequence: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> AccountConfig {
        serde_json::from_value(serde_json::json!({
            "id": "acc-2",
            "name": "Personagem #2",
            "enabled": true,
            "credentials": {"token": "abc"},
            "settings": {"auto_hunt": true},
            "role": "follower",
            "partyMembers": ["Lider", "Terceiro"],
            "partyBonusPercent": 45.0,
            "soloKillRate": 40.0
        }))
        .unwrap()
    }

    #[test]
    fn parses_party_fields() {
        let cfg = base_config();
        assert_eq!(cfg.role, Role::Follower);
        assert_eq!(cfg.party_members.len(), 2);
        assert_eq!(cfg.tier(), 0);

        let party = cfg.party_config();
        assert_eq!(party.role, Role::Follower);
        assert_eq!(party.members.len(), 2);
        assert!((party.bonus_percent - 45.0).abs() < 1e-9);
    }

    #[test]
    fn legacy_partner_name_becomes_party_member() {
        let mut cfg = base_config();
        cfg.role = Role::Leader;
        cfg.party_members.clear();
        cfg.partner_name = "Sozinho".into();
        assert_eq!(cfg.party_config().members, vec!["Sozinho".to_string()]);
    }

    #[test]
    fn detects_cookie_tokens() {
        let mut cfg = base_config();
        assert!(!cfg.credentials.token_is_cookie());
        cfg.credentials.token = "session=abc; cf_clearance=xyz".into();
        assert!(cfg.credentials.token_is_cookie());
    }
}
