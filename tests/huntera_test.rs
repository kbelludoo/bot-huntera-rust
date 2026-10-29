use bot_huntera_rust::game::cipher::xor_cipher;
use bot_huntera_rust::game::packet::{decode_packet, encode_packet};
use serde_json::json;

#[test]
fn test_huntera_cipher_and_packet_roundtrip() {
    let opcode = 205;
    let payload = json!({
        "action": "hunt_attack",
        "target": "dragon_lair",
        "power": 9000
    });
    let seed = 133742;

    let encoded = encode_packet(opcode, payload.clone(), seed).expect("Codificação do pacote falhou");
    assert!(encoded.len() > 5);

    let decoded = decode_packet(&encoded).expect("Decodificação do pacote falhou");
    assert_eq!(decoded.opcode, opcode);
    assert_eq!(decoded.payload.get("target").unwrap().as_str().unwrap(), "dragon_lair");
}
