use crate::game::cipher::{xor_cipher, COMPRESS_FLAG};
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use serde_json::{json, Value};
use std::io::{Read, Write};

#[derive(Debug, Clone)]
pub struct RawEvent {
    pub msg_type: String,
    pub opcode: u16,
    pub payload: Value,
}

pub fn encode_packet(opcode: u16, payload: Value, seed: u32) -> Result<Vec<u8>, String> {
    let packet_array = json!([opcode, payload]);
    let json_bytes = serde_json::to_vec(&packet_array).map_err(|e| format!("JSON encode: {e}"))?;

    let mut flags: u8 = 0;
    let mut data = json_bytes;

    if data.len() >= 8192 {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        if encoder.write_all(&data).is_ok() {
            if let Ok(compressed) = encoder.finish() {
                data = compressed;
                flags = COMPRESS_FLAG;
            }
        }
    }

    // Encrypt payload
    xor_cipher(&mut data, seed);

    let mut packet = Vec::with_capacity(5 + data.len());
    packet.push(flags);
    packet.extend_from_slice(&seed.to_le_bytes());
    packet.extend_from_slice(&data);

    Ok(packet)
}

pub fn decode_packet(buf: &[u8]) -> Result<RawEvent, String> {
    if buf.len() < 5 {
        return Err("Buffer muito curto para cabeçalho Huntera (mínimo 5 bytes)".to_string());
    }

    let flags = buf[0];
    let seed = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
    let mut encrypted = buf[5..].to_vec();

    xor_cipher(&mut encrypted, seed);

    let decompressed = if (flags & COMPRESS_FLAG) != 0 {
        let mut decoder = DeflateDecoder::new(&encrypted[..]);
        let mut dec = Vec::new();
        decoder.read_to_end(&mut dec).map_err(|e| format!("Deflate decompression error: {e}"))?;
        dec
    } else {
        encrypted
    };

    let parsed: Value = serde_json::from_slice(&decompressed).map_err(|e| format!("JSON decode: {e}"))?;

    let arr = parsed.as_array().ok_or_else(|| "Esperado array [opcode, payload]".to_string())?;
    let opcode = arr.get(0).and_then(|v| v.as_u64()).unwrap_or(0) as u16;
    let payload = arr.get(1).cloned().unwrap_or(Value::Null);

    Ok(RawEvent {
        msg_type: format!("opcode_{}", opcode),
        opcode,
        payload,
    })
}
