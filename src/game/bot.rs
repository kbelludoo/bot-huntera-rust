use crate::engine::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::engine::stamina::{StaminaConfig, StaminaController};
use crate::game::catalog::Catalog;
use crate::game::packet::{decode_packet, encode_packet};
use crate::game::types::AccountConfig;
use crate::subsystem::dailies::DailyManager;
use crate::subsystem::inventory::InventoryManager;
use crate::subsystem::potions::PotionManager;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

pub struct BotInstance {
    pub config: AccountConfig,
    pub circuit_breaker: Arc<CircuitBreaker>,
    pub stamina: Arc<StaminaController>,
    pub inventory: Arc<InventoryManager>,
    pub dailies: Arc<DailyManager>,
    pub potions: Arc<PotionManager>,
    pub catalog: Catalog,
    pub ws_sender: Arc<Mutex<Option<futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>>>>,
    pub kills: AtomicU64,
    pub gold: AtomicU64,
    pub exp: AtomicU64,
    pub current_level: AtomicU32,
    pub running: AtomicBool,
}

impl BotInstance {
    pub fn new(config: AccountConfig) -> Self {
        let cb_cfg = CircuitBreakerConfig::default();
        let cb = Arc::new(CircuitBreaker::new(&config.id, cb_cfg));
        let stam = Arc::new(StaminaController::new(StaminaConfig::default()));

        Self {
            config,
            circuit_breaker: cb,
            stamina: stam,
            inventory: Arc::new(InventoryManager::default()),
            dailies: Arc::new(DailyManager::default()),
            potions: Arc::new(PotionManager::default()),
            catalog: Catalog::default(),
            ws_sender: Arc::new(Mutex::new(None)),
            kills: AtomicU64::new(0),
            gold: AtomicU64::new(0),
            exp: AtomicU64::new(0),
            current_level: AtomicU32::new(1),
            running: AtomicBool::new(false),
        }
    }

    pub async fn send_game_packet(&self, opcode: u16, payload: serde_json::Value) -> Result<(), String> {
        let mut sender = self.ws_sender.lock().await;
        if let Some(s) = sender.as_mut() {
            let seed = rand::random::<u32>();
            let bin = encode_packet(opcode, payload, seed)?;
            s.send(Message::Binary(bin.into())).await.map_err(|e| format!("Falha de envio WS Huntera: {e}"))?;
            Ok(())
        } else {
            Err("WebSocket desconectado".to_string())
        }
    }

    pub async fn run_loop(self: Arc<Self>) {
        self.running.store(true, Ordering::SeqCst);
        info!("🚀 Bot [{}] (Huntera) iniciado com Cifra e Opcodes binários.", self.config.name);

        let origin = if self.config.credentials.origin.is_empty() {
            "https://huntera.net".to_string()
        } else {
            self.config.credentials.origin.clone()
        };

        if !self.config.credentials.token.is_empty() && self.config.credentials.token != "seu-token-de-sessao-ou-jwt" {
            let ws_scheme = if origin.starts_with("https") { "wss" } else { "ws" };
            let host = origin.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
            let ws_url = format!("{}://{}/ws", ws_scheme, host);

            if let Ok((ws_stream, _)) = connect_async(&ws_url).await {
                let (write, mut read) = ws_stream.split();
                *self.ws_sender.lock().await = Some(write);

                tokio::spawn(async move {
                    while let Some(msg) = read.next().await {
                        if let Ok(Message::Binary(bin)) = msg {
                            if let Ok(event) = decode_packet(&bin) {
                                debug!("[Huntera WS] Evento recebido: opcode={}", event.opcode);
                            }
                        }
                    }
                });
            }
        }

        let mut loop_counter = 0u64;

        while self.running.load(Ordering::SeqCst) {
            loop_counter += 1;

            if !self.circuit_breaker.can_execute().await {
                sleep(Duration::from_secs(5)).await;
                continue;
            }

            if !self.stamina.should_hunt().await {
                sleep(Duration::from_secs(5)).await;
                continue;
            }

            if self.config.settings.auto_daily && self.dailies.needs_claim() {
                self.dailies.record_claim();
                let _ = self.send_game_packet(101, json!({})).await;
                info!("[{}] Recompensas diárias do Huntera resgatadas.", self.config.name);
            }

            if self.config.settings.auto_hunt {
                let character_level = self.current_level.load(Ordering::Relaxed);
                let area = self.catalog.select_best_hunt(character_level);

                if loop_counter % 25 == 1 {
                    let _ = self.send_game_packet(200, json!({ "hunt": area.id })).await;
                    info!("🗡️ [{}] Huntera caçando na cave [{}]", self.config.name, area.id);
                }

                self.kills.fetch_add(1, Ordering::Relaxed);
                self.gold.fetch_add(area.base_gold as u64, Ordering::Relaxed);
                self.exp.fetch_add(area.base_exp as u64, Ordering::Relaxed);

                if self.config.settings.vip_farmer {
                    self.gold.fetch_add(15, Ordering::Relaxed);
                }

                self.circuit_breaker.record_success().await;
            }

            sleep(Duration::from_millis(1500)).await;
        }

        info!("🛑 Bot [{}] finalizado.", self.config.name);
    }
}
