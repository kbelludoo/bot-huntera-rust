use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

pub fn generate_idempotency_key() -> String {
    Uuid::new_v4().to_string()
}

pub fn sign_request(secret: &[u8], payload: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC pode receber chave de qualquer tamanho");
    mac.update(payload);
    hex::encode(mac.finalize().into_bytes())
}

pub fn compute_djb2_shard(s: &str) -> u32 {
    let mut hash: u32 = 5381;
    for b in s.bytes() {
        hash = ((hash << 5).wrapping_add(hash)).wrapping_add(b as u32);
    }
    hash
}
