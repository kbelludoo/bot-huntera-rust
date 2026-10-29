pub const ZE: u32 = 1213550164;
pub const COMPRESS_FLAG: u8 = 1;
pub const BATCH_FLAG: u8 = 2;

pub fn xor_cipher(r: &mut [u8], seed: u32) {
    let mut t = seed ^ ZE;
    if t == 0 {
        t = ZE;
    }

    let aligned_len = r.len() & !3;
    let mut a = 0;
    while a < aligned_len {
        t ^= t << 13;
        t ^= t >> 17;
        t ^= t << 5;
        let val = u32::from_le_bytes([r[a], r[a + 1], r[a + 2], r[a + 3]]);
        let xored = val ^ t;
        r[a..a + 4].copy_from_slice(&xored.to_le_bytes());
        a += 4;
    }

    if a < r.len() {
        t ^= t << 13;
        t ^= t >> 17;
        t ^= t << 5;
        while a < r.len() {
            r[a] ^= ((t >> ((a & 3) * 8)) & 0xFF) as u8;
            a += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xor_cipher_symmetry() {
        let mut data = b"Hello, Huntera Native Rust Engine!".to_vec();
        let original = data.clone();
        let seed = 429496729;

        xor_cipher(&mut data, seed);
        assert_ne!(data, original);

        xor_cipher(&mut data, seed);
        assert_eq!(data, original);
    }
}
