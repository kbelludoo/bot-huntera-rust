//! Enquadramento binário do protocolo Huntera.
//!
//! Layout de um frame (igual ao client oficial):
//! ```text
//! [ seed: u32 LE ][ flags: u8 ][ payload (JSON cifrado) ]
//! ```
//! A cifra XORShift roda sobre `seed..fim` (isto é, a partir do byte `flags`),
//! e o payload é um JSON `[opcode, {...}]`. Frames podem ser agrupados
//! (`BATCH_FLAG`) usando prefixo de tamanho `u32 LE` por sub-pacote.

use crate::game::cipher::{xor_cipher, BATCH_FLAG, COMPRESS_FLAG};
use crate::game::opcodes::{incoming_name, outgoing_code};
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use serde_json::{json, Value};
use std::io::{Read, Write};

/// Limite defensivo ao descomprimir (evita zip-bomb vindo de frame hostil).
const MAX_DECOMPRESSED: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct RawEvent {
    pub msg_type: String,
    pub opcode: u16,
    pub payload: Value,
}

impl RawEvent {
    pub fn is(&self, msg_type: &str) -> bool {
        self.msg_type == msg_type
    }
}

/// Codifica uma ação do jogo usando o nome oficial (`"start-hunt"`, ...).
pub fn encode_packet(msg_type: &str, payload: Value) -> Result<Vec<u8>, String> {
    encode_packet_with_seed(msg_type, payload, rand::random::<u32>())
}

/// Codifica com seed explícito (determinístico — usado em testes).
pub fn encode_packet_with_seed(
    msg_type: &str,
    payload: Value,
    seed: u32,
) -> Result<Vec<u8>, String> {
    let opcode = outgoing_code(msg_type)
        .ok_or_else(|| format!("Ação desconhecida no protocolo Huntera: {msg_type}"))?;

    let mut data = serde_json::to_vec(&json!([opcode, payload]))
        .map_err(|e| format!("JSON encode ({msg_type}): {e}"))?;

    let mut flags: u8 = 0;
    if data.len() >= 8192 {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(3));
        if encoder.write_all(&data).is_ok() {
            if let Ok(compressed) = encoder.finish() {
                data = compressed;
                flags |= COMPRESS_FLAG;
            }
        }
    }

    let mut out = Vec::with_capacity(5 + data.len());
    out.extend_from_slice(&seed.to_le_bytes());
    out.push(flags);
    out.extend_from_slice(&data);

    // Cifra flags + payload (offset 4), exatamente como o client oficial.
    xor_cipher(&mut out[4..], seed);

    Ok(out)
}

/// Decodifica um frame (suportando batch) em um ou mais eventos.
pub fn decode_frame(raw: &[u8]) -> Result<Vec<RawEvent>, String> {
    if raw.len() < 5 {
        return Err("Frame menor que o cabeçalho mínimo (5 bytes)".to_string());
    }

    let seed = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    let mut body = raw[4..].to_vec();
    xor_cipher(&mut body, seed);

    if body.is_empty() {
        return Err("Frame sem corpo após decifrar".to_string());
    }

    let flags = body[0];
    let content = &body[1..];

    if (flags & BATCH_FLAG) == 0 {
        let event = decode_single(content, flags)?;
        return Ok(vec![event]);
    }

    let mut events = Vec::new();
    let mut offset = 0usize;
    while offset + 4 <= content.len() {
        let len = u32::from_le_bytes([
            content[offset],
            content[offset + 1],
            content[offset + 2],
            content[offset + 3],
        ]) as usize;
        offset += 4;
        if len == 0 || offset + len > content.len() {
            break;
        }

        let sub = &content[offset..offset + len];
        // Sub-pacote repete o cabeçalho: seed + flags + payload.
        if sub.len() >= 5 {
            let sub_seed = u32::from_le_bytes([sub[0], sub[1], sub[2], sub[3]]);
            let mut sub_body = sub[4..].to_vec();
            xor_cipher(&mut sub_body, sub_seed);
            if !sub_body.is_empty() {
                if let Ok(ev) = decode_single(&sub_body[1..], sub_body[0]) {
                    events.push(ev);
                }
            }
        }
        offset += len;
    }

    Ok(events)
}

fn decode_single(payload: &[u8], flags: u8) -> Result<RawEvent, String> {
    let decoded = if flags & COMPRESS_FLAG != 0 {
        let mut out = Vec::new();
        DeflateDecoder::new(payload)
            .take(MAX_DECOMPRESSED)
            .read_to_end(&mut out)
            .map_err(|e| format!("Falha ao descomprimir frame: {e}"))?;
        out
    } else {
        payload.to_vec()
    };

    let parsed: Value =
        serde_json::from_slice(&decoded).map_err(|e| format!("JSON decode: {e}"))?;
    let arr = parsed
        .as_array()
        .ok_or_else(|| "Esperado array [opcode, payload]".to_string())?;
    if arr.len() < 2 {
        return Err("Array de pacote sem payload".to_string());
    }

    let opcode = arr[0]
        .as_u64()
        .ok_or_else(|| "Opcode não numérico".to_string())? as u16;

    let msg_type = incoming_name(opcode)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("unknown-{opcode}"));

    Ok(RawEvent {
        msg_type,
        opcode,
        payload: arr[1].clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_matches_official_layout() {
        let seed = 0xDEAD_BEEFu32;
        let frame = encode_packet_with_seed("ping", json!({"t": 1}), seed).unwrap();
        assert_eq!(&frame[0..4], &seed.to_le_bytes());
        // flags cifrado: decifrando offset 4 deve zerar
        let mut tail = frame[4..].to_vec();
        xor_cipher(&mut tail, seed);
        assert_eq!(tail[0], 0);
    }

    #[test]
    fn roundtrip_known_action() {
        let frame = encode_packet_with_seed(
            "party-invite-name",
            json!({"name": "Parceiro", "characterName": "Parceiro"}),
            12345,
        )
        .unwrap();
        let events = decode_frame(&frame).unwrap();
        assert_eq!(events.len(), 1);
        // 55 é `party-invite-name` no dicionário de ENTRADA? Não: entrada usa
        // códigos distintos. Aqui o decoder mapeia o opcode enviado caso exista.
        assert_eq!(events[0].opcode, 55);
    }

    #[test]
    fn rejects_unknown_action() {
        assert!(encode_packet("acao-inexistente", json!({})).is_err());
    }
}
