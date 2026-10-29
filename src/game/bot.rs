use crate::engine::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::engine::party::{PartyConfig, PartyCoordinator, Role, TeamState};
use crate::engine::stamina::{StaminaConfig, StaminaController, RESUME_STAMINA_MS};
use crate::game::catalog::Catalog;
use crate::game::opcodes::CLIENT_VERSION;
use crate::game::packet::{decode_frame, encode_packet, RawEvent};
use crate::game::types::AccountConfig;
use crate::subsystem::dailies::DailyManager;
use crate::subsystem::inventory::InventoryManager;
use crate::subsystem::potions::PotionManager;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration, Instant};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

type WsStream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WsSink = SplitSink<WsStream, Message>;

const DEFAULT_ORIGIN: &str = "https://huntera.com.br";
const DEFAULT_WS_PATH: &str = "/game-socket";
const PING_INTERVAL: Duration = Duration::from_secs(5);
const PARTY_FOLLOW_INTERVAL: Duration = Duration::from_secs(5);
const RECONNECT_DELAY: Duration = Duration::from_secs(8);

pub struct BotInstance {
    pub config: AccountConfig,
    pub role: Role,
    pub party: PartyCoordinator,
    pub team: Arc<TeamState>,
    pub circuit_breaker: Arc<CircuitBreaker>,
    pub stamina: Arc<StaminaController>,
    pub inventory: Arc<InventoryManager>,
    pub dailies: Arc<DailyManager>,
    pub potions: Arc<PotionManager>,
    pub catalog: Catalog,
    ws_sender: Mutex<Option<WsSink>>,
    pub connected: AtomicBool,
    pub in_hunt: AtomicBool,
    pub running: AtomicBool,
    pub kills: AtomicU64,
    pub gold: AtomicU64,
    pub exp: AtomicU64,
    pub current_level: AtomicU32,
    pub hp: AtomicI64,
    pub max_hp: AtomicI64,
    pub mana: AtomicI64,
    pub max_mana: AtomicI64,
    pub stamina_refill_cost: AtomicU64,
    pub stamina_refills_left: AtomicI64,
    pub latency_ms: AtomicU64,
    pub party_joins: AtomicU64,
    pub boss_runs: AtomicU64,
    pub char_name: RwLock<String>,
    pub vocation: RwLock<String>,
    pub current_hunt: RwLock<Option<String>>,
    pub last_event: RwLock<Option<String>>,
    pub last_error: RwLock<Option<String>>,
}

impl BotInstance {
    pub fn new(config: AccountConfig, team: Arc<TeamState>) -> Self {
        let party_cfg: PartyConfig = config.party_config();
        let role = party_cfg.role;
        let cb = Arc::new(CircuitBreaker::new(&config.id, CircuitBreakerConfig::default()));

        Self {
            role,
            party: PartyCoordinator::new(party_cfg),
            team,
            circuit_breaker: cb,
            stamina: Arc::new(StaminaController::new(StaminaConfig::default())),
            inventory: Arc::new(InventoryManager::default()),
            dailies: Arc::new(DailyManager::default()),
            potions: Arc::new(PotionManager::default()),
            catalog: Catalog::default(),
            ws_sender: Mutex::new(None),
            connected: AtomicBool::new(false),
            in_hunt: AtomicBool::new(false),
            running: AtomicBool::new(false),
            kills: AtomicU64::new(0),
            gold: AtomicU64::new(0),
            exp: AtomicU64::new(0),
            current_level: AtomicU32::new(1),
            hp: AtomicI64::new(0),
            max_hp: AtomicI64::new(0),
            mana: AtomicI64::new(0),
            max_mana: AtomicI64::new(0),
            stamina_refill_cost: AtomicU64::new(0),
            stamina_refills_left: AtomicI64::new(0),
            latency_ms: AtomicU64::new(0),
            party_joins: AtomicU64::new(0),
            boss_runs: AtomicU64::new(0),
            char_name: RwLock::new(config.name.clone()),
            vocation: RwLock::new(String::from("Aventureiro")),
            current_hunt: RwLock::new(None),
            last_event: RwLock::new(None),
            last_error: RwLock::new(None),
            config,
        }
    }

    pub fn name(&self) -> String {
        self.char_name
            .try_read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| self.config.name.clone())
    }

    pub fn party_snapshot(&self) -> crate::engine::party::PartySnapshot {
        self.party.snapshot()
    }

    /// URL do socket do jogo derivada de `credentials.origin`.
    pub fn ws_url(&self) -> Result<String, String> {
        let raw = self.config.credentials.origin.trim();
        let origin = if raw.is_empty() { DEFAULT_ORIGIN } else { raw };
        let parsed = url::Url::parse(origin).map_err(|e| format!("origin inválida ({origin}): {e}"))?;

        let scheme = match parsed.scheme() {
            "https" | "wss" => "wss",
            "http" | "ws" => "ws",
            other => return Err(format!("scheme não suportado: {other}")),
        };
        let host = parsed.host_str().ok_or("origin sem host")?;

        let path = if parsed.path().contains("game") {
            parsed.path().to_string()
        } else {
            DEFAULT_WS_PATH.to_string()
        };
        let port = parsed.port().map(|p| format!(":{p}")).unwrap_or_default();

        Ok(format!("{scheme}://{host}{port}{path}"))
    }

    async fn send_action(&self, msg_type: &str, payload: Value) -> Result<(), String> {
        let mut guard = self.ws_sender.lock().await;
        let sink = guard.as_mut().ok_or("WebSocket desconectado")?;

        let frame = encode_packet(msg_type, payload)?;
        sink.send(Message::Binary(frame.into()))
            .await
            .map_err(|e| format!("Falha de envio WS ({msg_type}): {e}"))?;
        debug!("[{}] → {}", self.config.id, msg_type);
        Ok(())
    }

    async fn send_optional(&self, msg_type: &str, payload: Value) {
        if let Err(e) = self.send_action(msg_type, payload).await {
            self.set_error(&e).await;
            warn!("[{}] {}", self.config.id, e);
        }
    }

    async fn set_error(&self, e: &str) {
        *self.last_error.write().await = Some(e.to_string());
    }

    pub async fn run_loop(self: Arc<Self>) {
        self.running.store(true, Ordering::SeqCst);
        info!(
            "[{}] 🚀 Huntera bot iniciado (role={}, party={} membros, alvo {:.2} kills/min)",
            self.config.id,
            self.role.as_str(),
            self.party.config().members.len() + 1,
            self.party.kill_rate_per_min()
        );

        while self.running.load(Ordering::SeqCst) {
            if !self.circuit_breaker.can_execute().await {
                sleep(Duration::from_secs(10)).await;
                continue;
            }

            match self.connect_once().await {
                Ok(()) => {
                    info!("[{}] ✅ Conectado ao jogo como {}.", self.config.id, self.name());
                    self.action_loop().await;
                    let _ = self.disconnect().await;
                    warn!("[{}] 🔌 WS encerrado. Reconectando em {:?}...", self.config.id, RECONNECT_DELAY);
                }
                Err(e) => {
                    self.set_error(&e).await;
                    warn!("[{}] ⚠️ Falha ao conectar: {}", self.config.id, e);
                    self.circuit_breaker.record_failure(&e).await;
                }
            }

            sleep(RECONNECT_DELAY).await;
        }

        info!("[{}] 🛑 Bot finalizado.", self.config.id);
    }

    async fn connect_once(self: &Arc<Self>) -> Result<(), String> {
        let ws_url = self.ws_url()?;

        let mut request = ws_url
            .as_str()
            .into_client_request()
            .map_err(|e| format!("Handshake WS inválido: {e}"))?;

        let origin = if self.config.credentials.origin.trim().is_empty() {
            DEFAULT_ORIGIN.to_string()
        } else {
            self.config.credentials.origin.clone()
        };

        let headers = request.headers_mut();
        headers.insert("User-Agent", HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36"));
        headers.insert("Origin", HeaderValue::from_str(&origin).map_err(|e| e.to_string())?);
        if self.config.credentials.token_is_cookie() {
            headers.insert(
                "Cookie",
                HeaderValue::from_str(&self.config.credentials.token).map_err(|e| e.to_string())?,
            );
        }

        let (stream, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| format!("connect_async: {e}"))?;

        let (sink, mut read) = stream.split();
        *self.ws_sender.lock().await = Some(sink);
        self.connected.store(true, Ordering::SeqCst);

        self.send_action(
            "authenticate",
            json!({ "clientVersion": CLIENT_VERSION, "ticket": self.config.credentials.token }),
        )
        .await?;

        // Loop de leitura de eventos do servidor.
        let reader = Arc::clone(self);
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Binary(bin)) => match decode_frame(&bin) {
                        Ok(events) => {
                            for ev in events {
                                reader.handle_event(ev).await;
                            }
                        }
                        Err(e) => debug!("[{}] Frame inválido: {}", reader.config.id, e),
                    },
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(e) => {
                        debug!("[{}] Erro de leitura WS: {}", reader.config.id, e);
                        break;
                    }
                }
            }
            reader.connected.store(false, Ordering::SeqCst);
        });

        // Ping keep-alive (obrigatório: servidor derruba socket ocioso).
        let pinger = Arc::clone(self);
        tokio::spawn(async move {
            while pinger.connected.load(Ordering::SeqCst) && pinger.running.load(Ordering::SeqCst) {
                let sent_at = chrono::Utc::now().timestamp_millis();
                if pinger
                    .send_action("ping", json!({ "t": sent_at }))
                    .await
                    .is_err()
                {
                    break;
                }
                sleep(PING_INTERVAL).await;
            }
        });

        // Configura action bar gerenciada + preset por vocação (rotação de skills).
        self.send_optional("set-action-bar-managed", json!({ "managed": true }))
            .await;
        let preset = match self.config.id.as_str() {
            "acc3" => "knight",
            "acc2" => "druid",
            "acc4" => "sorcerer",
            _ => "paladin",
        };
        self.send_optional("select-action-bar-preset", json!({ "preset": preset }))
            .await;

        self.circuit_breaker.record_success().await;
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), String> {
        self.connected.store(false, Ordering::SeqCst);
        self.in_hunt.store(false, Ordering::SeqCst);
        if let Some(mut sink) = self.ws_sender.lock().await.take() {
            let _ = sink.send(Message::Close(None)).await;
            let _ = sink.close().await;
        }
        Ok(())
    }

    async fn action_loop(self: &Arc<Self>) {
        self.stamina.force_resume();
        let mut last_follow = Instant::now() - PARTY_FOLLOW_INTERVAL;

        while self.running.load(Ordering::SeqCst) && self.connected.load(Ordering::SeqCst) {
            if !self.circuit_breaker.can_execute().await {
                sleep(Duration::from_secs(10)).await;
                continue;
            }

            let epoch = self.team.rest_epoch();

            // ─── Regras de stamina/fadiga ───────────────────────────────
            if !self.stamina.should_hunt().await {
                self.stamina.force_rest();
                let reason = format!(
                    "{} com {:.1}h de stamina",
                    self.name(),
                    self.stamina.stamina_hours()
                );
                if self.team.begin_rest(reason.clone()) {
                    warn!(
                        "[{}] 😴 Recuo coletivo iniciado: {}. Time inteiro vai treinar.",
                        self.config.id, reason
                    );
                }
                if self.in_hunt.swap(false, Ordering::SeqCst) {
                    self.send_optional("leave-hunt", json!({})).await;
                }
                self.set_resting_actions(true).await;
                sleep(Duration::from_secs(10)).await;
                continue;
            }

            if self.team.is_resting() {
                if self.stamina.stamina_ms().unwrap_or(RESUME_STAMINA_MS) < RESUME_STAMINA_MS {
                    self.set_resting_actions(true).await;
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
                if self.team.mark_recovered(epoch) {
                    info!(
                        "[{}] 💪 Time inteiro recuperado! Retomando caçada em party.",
                        self.config.id
                    );
                    self.set_resting_actions(false).await;
                } else {
                    self.set_resting_actions(true).await;
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
            }

            // ─── Recompensas diárias ───────────────────────────────────
            if self.config.settings.auto_daily && self.dailies.needs_claim() {
                self.dailies.record_claim();
                self.send_optional("claim-daily-reward", json!({})).await;
                info!("[{}] 🎁 Recompensa diária resgatada.", self.config.id);
            }

            // ─── Party / caçada ────────────────────────────────────────
            match self.role {
                Role::Leader => {
                    self.lead_party().await;
                    if self.config.settings.auto_hunt {
                        self.ensure_hunting().await;
                    }
                }
                Role::Follower => {
                    // Seguidor NUNCA inicia caçada sozinho: entra na sala do líder.
                    if !self.in_hunt.load(Ordering::SeqCst)
                        && last_follow.elapsed() >= PARTY_FOLLOW_INTERVAL
                    {
                        last_follow = Instant::now();
                        self.party.note_follow();
                        self.send_optional("party-follow-leader", json!({})).await;
                    }
                }
                Role::Solo => {
                    if self.config.settings.auto_hunt {
                        self.ensure_hunting().await;
                    }
                }
            }

            sleep(self.party.action_interval()).await;
        }
    }

    async fn set_resting_actions(&self, resting: bool) {
        if resting {
            self.send_optional("set-idle-training", json!({ "enabled": true }))
                .await;
            // Refill automático por ouro se o custo couber no caixa com folga.
            let cost = self.stamina_refill_cost.load(Ordering::Relaxed);
            let refills = self.stamina_refills_left.load(Ordering::Relaxed);
            let gold = self.gold.load(Ordering::Relaxed);
            if cost > 0 && refills != 0 && gold >= cost.saturating_mul(3) {
                self.send_optional("buy-stamina", json!({ "cost": cost })).await;
                info!(
                    "[{}] 🧪 Refill de stamina comprado ({} gp).",
                    self.config.id, cost
                );
            }
        } else {
            self.send_optional("set-idle-training", json!({ "enabled": false }))
                .await;
        }
    }

    async fn lead_party(&self) {
        if self.party.config().members.is_empty() {
            return;
        }
        if !self.party.should_invite_now() {
            return;
        }

        for member in self.party.config().members.clone() {
            self.party.note_invite();
            self.send_optional(
                "party-invite-name",
                json!({ "name": member, "characterName": member }),
            )
            .await;
            debug!("[{}] 🤝 Convite de party enviado para {}", self.config.id, member);
        }
    }

    async fn ensure_hunting(&self) {
        let level = self.current_level.load(Ordering::Relaxed);
        let area = if self.config.settings.hunt_target.is_empty() {
            self.catalog.select_best_hunt(level).id
        } else {
            self.config.settings.hunt_target.as_str()
        };

        let already = self
            .current_hunt
            .read()
            .await
            .clone()
            .map(|h| h == area)
            .unwrap_or(false);

        let tier = self.config.tier();

        if self.in_hunt.load(Ordering::SeqCst) {
            if !already {
                self.send_optional(
                    "change-hunt",
                    json!({ "huntId": area, "tier": tier }),
                )
                .await;
                *self.current_hunt.write().await = Some(area.to_string());
                info!("[{}] 🔁 Trocou de caçada para [{}]", self.config.id, area);
            }
            return;
        }

        self.send_optional("start-hunt", json!({ "huntId": area, "tier": tier }))
            .await;
        self.in_hunt.store(true, Ordering::SeqCst);
        *self.current_hunt.write().await = Some(area.to_string());
        info!(
            "[{}] 🗡️ Caçada iniciada em [{}] (party={} · {:.2} kills/min)",
            self.config.id,
            area,
            self.party.party_size(),
            self.party.kill_rate_per_min()
        );
    }

    // ───────────────────────── Handlers de eventos ─────────────────────────

    async fn handle_event(&self, ev: RawEvent) {
        *self.last_event.write().await = Some(ev.msg_type.clone());
        debug!("[{}] ← {} {}", self.config.id, ev.msg_type, ev.payload);

        match ev.msg_type.as_str() {
            "pong" => {
                if let Some(t) = ev.payload.get("t").and_then(|v| v.as_i64()) {
                    let delta = chrono::Utc::now().timestamp_millis() - t;
                    if delta >= 0 {
                        self.latency_ms.store(delta as u64, Ordering::Relaxed);
                    }
                }
            }

            // ── Party ────────────────────────────────────────────────
            "party-invited" | "party-invite-confirm" => {
                info!("[{}] 🤝 Convite de party recebido. Aceitando.", self.config.id);
                self.party_joins.fetch_add(1, Ordering::Relaxed);
                self.party.set_in_party(true);
                self.send_optional("party-respond", json!({ "accept": true })).await;
                self.send_optional("party-follow-leader", json!({})).await;
                self.party.note_follow();
            }
            "party-update" => {
                let size = extract_party_size(&ev.payload)
                    .unwrap_or(self.party.config().members.len() + 1);
                let leader = extract_str(&ev.payload, &["leader", "leaderName"]);
                self.party.apply_party_update(size, leader.clone());
                info!(
                    "[{}] 👥 Party atualizada: {} membros (líder: {}) · {:.2} kills/min",
                    self.config.id,
                    self.party.party_size(),
                    leader.unwrap_or_else(|| "?".into()),
                    self.party.kill_rate_per_min()
                );
                if self.role.is_leader() {
                    self.send_optional("party-costs-offer", json!({})).await;
                }
            }
            "hunt-team-invited" | "hunt-leader-entered" => {
                info!("[{}] 🏹 Entrada na caçada do líder confirmada.", self.config.id);
                self.send_optional("hunt-team-respond", json!({ "accept": true }))
                    .await;
                self.send_optional("party-follow-leader", json!({})).await;
                self.party.note_follow();
                self.in_hunt.store(true, Ordering::SeqCst);
            }
            "hunt-leader-invite-cancelled" | "hunt-team-invite-cancelled" => {
                self.in_hunt.store(false, Ordering::SeqCst);
            }
            "hunt-leave-pending" => {
                self.in_hunt.store(false, Ordering::SeqCst);
                *self.current_hunt.write().await = None;
            }
            "party-costs-offered" => {
                info!("[{}] 💰 Divisão de custos da party aceita.", self.config.id);
                self.party.set_costs_accepted(true);
                self.send_optional("party-costs-respond", json!({ "accept": true }))
                    .await;
            }
            "party-costs-cancelled" | "party-costs-settled" => {
                self.party.set_costs_accepted(false);
            }

            // ── Daily Boss (party de 4 vocações) ─────────────────────
            "daily-boss-status" => {
                if self.role.is_leader() && boss_available(&ev.payload) {
                    info!("[{}] 🐉 Chefe diário disponível! Criando sala.", self.config.id);
                    self.boss_runs.fetch_add(1, Ordering::Relaxed);
                    self.send_optional("daily-boss-create", json!({ "tier": 0 })).await;
                    for member in self.party.config().members.clone() {
                        self.send_optional(
                            "daily-boss-invite",
                            json!({ "name": member, "characterName": member }),
                        )
                        .await;
                    }
                }
            }
            "daily-boss-invited" => {
                let room = ev
                    .payload
                    .get("roomId")
                    .or_else(|| ev.payload.get("room"))
                    .cloned()
                    .unwrap_or(Value::Null);
                info!("[{}] 🐉 Convite de boss recebido. Entrando e marcando pronto.", self.config.id);
                self.send_optional("daily-boss-join", json!({ "roomId": room })).await;
                sleep(Duration::from_millis(300)).await;
                self.send_optional("daily-boss-ready", json!({ "ready": true })).await;
            }
            "daily-boss-rooms" | "daily-boss-room" => {
                if self.role.is_leader() && is_room_ready(&ev.payload) {
                    info!("[{}] 🐉 Party pronta! Iniciando confronto.", self.config.id);
                    self.send_optional("daily-boss-start", json!({})).await;
                }
            }
            "daily-boss-rewards" | "daily-boss-victory" => {
                info!("[{}] 🏆 Recompensa do boss resgatada.", self.config.id);
                self.send_optional("daily-boss-claim", json!({})).await;
            }

            // ── Progressão / estado ──────────────────────────────────
            "player-stats" | "welcome" => self.apply_player_stats(&ev.payload).await,
            "player-vitals" => self.apply_vitals(&ev.payload).await,
            "experience-gain" | "hunt-analyzer-update" => {
                let gain = extract_num(&ev.payload, &["value", "exp", "experience", "xp"])
                    .unwrap_or(0);
                if gain > 0 {
                    self.exp.fetch_add(gain as u64, Ordering::Relaxed);
                }
                self.kills.fetch_add(1, Ordering::Relaxed);
            }
            "bestiary-progress" => {
                if let Some(total) = extract_num(&ev.payload, &["totalKills", "total", "kills"]) {
                    self.kills.fetch_max(total.max(0) as u64, Ordering::Relaxed);
                }
                self.kills.fetch_add(1, Ordering::Relaxed);
            }
            "coins" => {
                if let Some(gold) = extract_num(&ev.payload, &["gold", "coins", "value", "balance"])
                {
                    if gold >= 0 {
                        self.gold.store(gold as u64, Ordering::Relaxed);
                    }
                }
            }
            "instance-enter" | "hunt-portal-entered" => {
                self.in_hunt.store(true, Ordering::SeqCst);
                if let Some(hunt) = extract_str(&ev.payload, &["huntId", "hunt"]) {
                    *self.current_hunt.write().await = Some(hunt);
                }
            }
            "player-died" => {
                self.in_hunt.store(false, Ordering::SeqCst);
                warn!("[{}] ☠️ Personagem morreu. Revivendo e recuando.", self.config.id);
                self.send_optional("leave-hunt", json!({})).await;
                self.send_optional("revive", json!({})).await;
            }

            // ── Quests / diárias ─────────────────────────────────────
            "daily-reward-offer" => {
                if self.config.settings.auto_daily {
                    self.dailies.record_claim();
                    self.send_optional("claim-daily-reward", json!({})).await;
                }
            }
            "quest-status" => self.handle_quests(&ev.payload).await,

            // ── Forja / prey ─────────────────────────────────────────
            "forge-state" => {
                self.send_optional("forge-auto-convert", json!({ "enabled": true }))
                    .await;
            }
            "forge-dust-gained" => {
                let dust = extract_num(&ev.payload, &["dust"]).unwrap_or(0);
                let cap = extract_num(&ev.payload, &["cap"]).unwrap_or(0);
                if dust >= 100 || (cap > 0 && dust >= cap - 20) {
                    self.send_optional("forge-convert", json!({ "amount": dust })).await;
                }
            }
            "prey-state" => self.handle_prey(&ev.payload).await,

            // ── Anti-cheat heurístico ────────────────────────────────
            "system-message" | "broadcast" => {
                let text = ev.payload.to_string().to_lowercase();
                if is_anti_cheat_text(&text) {
                    warn!(
                        "[{}] 🚨 Sinal anti-cheat detectado no servidor. Desarmando.",
                        self.config.id
                    );
                    self.in_hunt.store(false, Ordering::SeqCst);
                    self.send_optional("leave-hunt", json!({})).await;
                    self.send_optional("set-idle-training", json!({ "enabled": true }))
                        .await;
                    self.circuit_breaker.record_failure("rejeição anti-cheat").await;
                } else if text.contains("party") && text.contains("convite") {
                    self.send_optional("party-respond", json!({ "accept": true })).await;
                }
            }
            _ => {}
        }
    }

    async fn apply_player_stats(&self, payload: &Value) {
        if let Some(level) = extract_num(payload, &["level"]) {
            if level > 0 {
                self.current_level.store(level as u32, Ordering::Relaxed);
            }
        }
        if let Some(exp) = extract_num(payload, &["experience", "exp", "xp"]) {
            if exp > 0 {
                self.exp.store(exp as u64, Ordering::Relaxed);
            }
        }
        if let Some(name) = extract_str(payload, &["name", "characterName", "charName"]) {
            *self.char_name.write().await = name;
        }
        if let Some(voc) = extract_str(payload, &["vocation", "className", "profession"]) {
            *self.vocation.write().await = voc;
        }
        if let Some(stamina) = extract_num(payload, &["staminaMs", "stamina"]) {
            self.stamina.update_stamina_ms(stamina);
        }
        if let Some(cost) = extract_num(payload, &["staminaRefillCost"]) {
            self.stamina_refill_cost.store(cost.max(0) as u64, Ordering::Relaxed);
        }
        if let Some(left) = extract_num(payload, &["staminaRefillsLeft"]) {
            self.stamina_refills_left.store(left, Ordering::Relaxed);
        }
        if let Some(hp) = extract_num(payload, &["health", "hp"]) {
            self.hp.store(hp, Ordering::Relaxed);
        }
        if let Some(hp) = extract_num(payload, &["maxHealth", "maxHp"]) {
            self.max_hp.store(hp, Ordering::Relaxed);
        }
        if let Some(m) = extract_num(payload, &["mana"]) {
            self.mana.store(m, Ordering::Relaxed);
        }
        if let Some(m) = extract_num(payload, &["maxMana"]) {
            self.max_mana.store(m, Ordering::Relaxed);
        }
        self.circuit_breaker.record_success().await;
    }

    async fn apply_vitals(&self, payload: &Value) {
        if let Some(hp) = extract_num(payload, &["health", "hp"]) {
            self.hp.store(hp, Ordering::Relaxed);
            if hp <= 0 {
                self.in_hunt.store(false, Ordering::SeqCst);
                self.send_optional("cancel-hunt", json!({})).await;
                self.send_optional("revive", json!({})).await;
                return;
            }
        }
        if let Some(m) = extract_num(payload, &["mana"]) {
            self.mana.store(m, Ordering::Relaxed);
        }

        let hp = self.hp.load(Ordering::Relaxed);
        let max_hp = self.max_hp.load(Ordering::Relaxed);
        if max_hp > 0
            && hp > 0
            && (hp as f64 / max_hp as f64) < 0.25
            && self.in_hunt.load(Ordering::SeqCst)
        {
            warn!("[{}] 🩸 HP < 25%: recuo tático para regenerar.", self.config.id);
            self.in_hunt.store(false, Ordering::SeqCst);
            self.send_optional("leave-hunt", json!({})).await;
        }
    }

    async fn handle_quests(&self, payload: &Value) {
        let quests = payload.get("quests").and_then(|q| q.as_array());
        if let Some(list) = quests {
            for q in list {
                let claimable = q
                    .get("canClaim")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                    || q
                        .get("completed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    || q.get("status").and_then(|v| v.as_str()) == Some("completed");
                if claimable {
                    let id = q.get("id").cloned().unwrap_or(Value::Null);
                    self.send_optional("quest-claim", json!({ "questId": id })).await;
                }
            }
        }
        if let Some(available) = payload.get("available").and_then(|a| a.as_array()) {
            for q in available {
                let id = q.get("id").cloned().unwrap_or(Value::Null);
                self.send_optional("quest-pick", json!({ "questId": id })).await;
            }
        }
    }

    async fn handle_prey(&self, payload: &Value) {
        let Some(slots) = payload.get("slots").and_then(|s| s.as_array()) else {
            return;
        };
        for (idx, slot) in slots.iter().enumerate() {
            let state = slot.get("state").and_then(|s| s.as_str()).unwrap_or("");
            let free_reroll = slot
                .get("freeReroll")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let inactive = matches!(state, "list-selection" | "inactive" | "empty");
            if inactive && free_reroll {
                let slot_idx = slot
                    .get("index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(idx as u64);
                self.send_optional("prey-list-reroll", json!({ "slot": slot_idx }))
                    .await;
            }
        }
    }
}

// ───────────────────────── Helpers de payload ─────────────────────────

fn extract_num(payload: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(v) = payload.get(*key) {
            if let Some(n) = v.as_i64() {
                return Some(n);
            }
            if let Some(n) = v.as_f64() {
                return Some(n as i64);
            }
        }
    }
    // Alguns eventos aninham o estado em `player`/`status`.
    for nested in ["player", "status", "character", "data"] {
        if let Some(obj) = payload.get(nested) {
            if obj.is_object() {
                if let Some(n) = extract_num(obj, keys) {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn extract_str(payload: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(v) = payload.get(*key).and_then(|v| v.as_str()) {
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    for nested in ["player", "status", "character", "party"] {
        if let Some(obj) = payload.get(nested) {
            if obj.is_object() {
                if let Some(v) = extract_str(obj, keys) {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn extract_party_size(payload: &Value) -> Option<usize> {
    if let Some(arr) = payload.get("members").and_then(|v| v.as_array()) {
        return Some(arr.len().max(1));
    }
    if let Some(n) = payload.get("size").and_then(|v| v.as_u64()) {
        return Some(n.max(1) as usize);
    }
    payload
        .get("party")
        .and_then(|p| p.get("members"))
        .and_then(|v| v.as_array())
        .map(|a| a.len().max(1))
}

fn boss_available(payload: &Value) -> bool {
    let flag = |k: &str| payload.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let cooldown = payload
        .get("cooldownMs")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    flag("canEnter") || flag("available") || cooldown <= 0
}

fn is_room_ready(payload: &Value) -> bool {
    payload
        .get("ready")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || payload
            .get("allReady")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
}

/// Heurísticas de rejeição que exigem desarme imediato (regra anti-ban).
pub fn is_anti_cheat_text(text: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "experiência rápida demais",
        "experiencia rapida demais",
        "pontos de atributo demais",
        "save malformado",
        "outra aba",
        "outraaba",
        "banido",
        "suspens",
        "\"recusado\":true",
        "\"recusado\": true",
        "too many requests",
    ];
    NEEDLES.iter().any(|n| text.contains(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_nested_stats() {
        let payload = json!({
            "player": { "level": 104, "health": 900, "maxHealth": 1200, "staminaMs": 3600000 },
            "experience": 123
        });
        assert_eq!(extract_num(&payload, &["level"]), Some(104));
        assert_eq!(extract_num(&payload, &["maxHealth"]), Some(1200));
        assert_eq!(extract_num(&payload, &["experience"]), Some(123));
    }

    #[test]
    fn party_size_from_members_or_size() {
        assert_eq!(extract_party_size(&json!({"members": ["a","b","c"]})), Some(3));
        assert_eq!(extract_party_size(&json!({"party": {"members": ["a","b"]}})), Some(2));
        assert_eq!(extract_party_size(&json!({"size": 4})), Some(4));
        assert_eq!(extract_party_size(&json!({})), None);
    }

    #[test]
    fn detects_anti_cheat_messages() {
        assert!(is_anti_cheat_text("experiência rápida demais para este personagem"));
        assert!(is_anti_cheat_text(r#"{"outraAba":true}"#));
        assert!(!is_anti_cheat_text("bem-vindo ao huntera"));
    }

    #[test]
    fn boss_availability_heuristics() {
        assert!(boss_available(&json!({"canEnter": true})));
        assert!(boss_available(&json!({"cooldownMs": 0})));
        assert!(!boss_available(&json!({"cooldownMs": 60000})));
    }

    #[test]
    fn leader_role_never_starts_hunt_as_follower() {
        let cfg: AccountConfig = serde_json::from_value(json!({
            "id": "acc-2",
            "name": "Follower",
            "enabled": true,
            "credentials": {"token": "t"},
            "role": "follower",
            "partyMembers": ["Lider"]
        }))
        .unwrap();
        let bot = BotInstance::new(cfg, Arc::new(TeamState::new(2)));
        assert_eq!(bot.role, Role::Follower);
        assert_eq!(bot.party.party_size(), 1);
        assert_eq!(bot.ws_url().unwrap(), "wss://huntera.com.br/game-socket");
    }
}
