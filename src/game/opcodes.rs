//! Tabela oficial de opcodes do Huntera (eventos de entrada e ações de saída).
//!
//! O servidor fala `[opcode, payload]` em JSON cifrado, onde cada ação tem um
//! nome estável (`"party-invite-name"`, `"start-hunt"`, ...). Este módulo mantém
//! a tradução nome <-> código carregada uma única vez (sem alocação em runtime).

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

pub const CLIENT_VERSION: &str = "0.3.0+e0";

const OPCODES_JSON: &str = include_str!("opcodes.json");

#[derive(Debug, Deserialize)]
struct OpcodesData {
    incoming: HashMap<String, u16>,
    outgoing: HashMap<String, u16>,
}

struct Tables {
    outgoing: HashMap<String, u16>,
    incoming: HashMap<u16, String>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let data: OpcodesData = serde_json::from_str(OPCODES_JSON).unwrap_or(OpcodesData {
            incoming: HashMap::new(),
            outgoing: HashMap::new(),
        });

        let mut incoming = HashMap::with_capacity(data.incoming.len());
        for (name, code) in data.incoming {
            incoming.insert(code, name);
        }

        Tables {
            outgoing: data.outgoing,
            incoming,
        }
    })
}

/// Código de envio para uma ação do jogo (ex.: `"party-respond"` -> `57`).
pub fn outgoing_code(msg_type: &str) -> Option<u16> {
    tables().outgoing.get(msg_type).copied()
}

/// Nome do evento de entrada a partir do código recebido.
pub fn incoming_name(code: u16) -> Option<&'static str> {
    tables().incoming.get(&code).map(|s| s.as_str())
}

/// Quantidade de ações conhecidas (útil para auditoria/diagnóstico).
pub fn outgoing_len() -> usize {
    tables().outgoing.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_party_and_hunt_actions() {
        assert_eq!(outgoing_code("party-invite-name"), Some(55));
        assert_eq!(outgoing_code("party-follow-leader"), Some(172));
        assert_eq!(outgoing_code("party-respond"), Some(57));
        assert_eq!(outgoing_code("hunt-team-respond"), Some(35));
        assert_eq!(outgoing_code("start-hunt"), Some(89));
        assert_eq!(outgoing_code("daily-boss-create"), Some(139));
    }

    #[test]
    fn resolves_incoming_events() {
        assert_eq!(incoming_name(71), Some("party-invited"));
        assert_eq!(incoming_name(72), Some("party-update"));
        assert_eq!(incoming_name(44), Some("hunt-leader-entered"));
        assert_eq!(incoming_name(139), Some("party-costs-offered"));
        assert!(outgoing_len() > 100);
    }
}
