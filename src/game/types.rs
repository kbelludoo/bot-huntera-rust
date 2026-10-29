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

