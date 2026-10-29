#![allow(dead_code)]
#![allow(unused_imports)]

mod engine;
mod network;
mod subsystem;
mod game;
mod server;

use engine::supervisor::Supervisor;
use game::types::AccountConfig;
use server::dashboard::WebServer;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::signal;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

const DEFAULT_ACCOUNTS_EXAMPLE: &str = include_str!("../accounts.example.json");

fn format_number(n: u64) -> String {
    if n < 1000 {
        format!("{n}")
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        format!("{:.2}kk", n as f64 / 1_000_000.0)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    println!("================================================================================");
    println!("       ⚡ HUNTERA NATIVE RUST BOT (ZERO-GC & ALTA PERFORMANCE) ⚡");
    println!("================================================================================");

    let config_path = "accounts.json";
    let content = match std::fs::read_to_string(config_path) {
        Ok(data) => data,
        Err(_) => {
            warn!("Arquivo accounts.json não encontrado. Criando template padrão...");
            let _ = std::fs::write(config_path, DEFAULT_ACCOUNTS_EXAMPLE);
            DEFAULT_ACCOUNTS_EXAMPLE.to_string()
        }
    };

    let all_configs: Vec<AccountConfig> = match serde_json::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            error!("Falha crítica ao analisar accounts.json: {}", e);
            std::process::exit(1);
        }
    };

    let target_acc = std::env::var("ACCOUNT").unwrap_or_default();
    let mut active_configs = Vec::new();

    for a in all_configs {
        if !a.enabled && a.id != target_acc {
            continue;
        }
        if !target_acc.is_empty() && a.id != target_acc {
            continue;
        }
        active_configs.push(a);
    }

    if active_configs.is_empty() {
        warn!("Nenhuma conta com 'enabled: true' encontrada em accounts.json.");
        info!("Edite accounts.json e habilite sua conta para iniciar o bot.");
    }

    let supervisor = Arc::new(Supervisor::new(active_configs));
    supervisor.start_all().await;

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let web_server = WebServer::new(supervisor.clone(), port);
    tokio::spawn(async move {
        if let Err(e) = web_server.run().await {
            error!("Erro no servidor web: {}", e);
        }
    });

    let sup_clone = supervisor.clone();
    tokio::spawn(async move {
        let mut tick = 0u64;
        loop {
            sleep(Duration::from_secs(4)).await;
            tick += 1;

            let mut total_kills = 0u64;
            let mut total_gold = 0u64;
            let mut total_exp = 0u64;
            let active_bots = sup_clone.bots();

            for b in active_bots {
                total_kills += b.kills.load(Ordering::Relaxed);
                total_gold += b.gold.load(Ordering::Relaxed);
                total_exp += b.exp.load(Ordering::Relaxed);
            }

            info!(
                "[Console Ticker #{}] Contas: {} | Kills: {} | Gold: {} | Exp: {}",
                tick,
                active_bots.len(),
                total_kills,
                format_number(total_gold),
                format_number(total_exp)
            );
        }
    });

    info!("Bot operacional na porta {}. Pressione Ctrl+C para encerrar.", port);
    signal::ctrl_c().await?;
    info!("Encerrando bot...");

    Ok(())
}
