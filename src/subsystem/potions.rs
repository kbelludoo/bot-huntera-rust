pub struct PotionManager {
    pub hp_threshold: f64,
    pub mana_threshold: f64,
}

impl Default for PotionManager {
    fn default() -> Self {
        Self {
            hp_threshold: 0.40,
            mana_threshold: 0.25,
        }
    }
}

impl PotionManager {
    pub fn needs_hp_pot(&self, current: u32, max: u32) -> bool {
        (current as f64 / max.max(1) as f64) <= self.hp_threshold
    }

    pub fn needs_mana_pot(&self, current: u32, max: u32) -> bool {
        (current as f64 / max.max(1) as f64) <= self.mana_threshold
    }
}
