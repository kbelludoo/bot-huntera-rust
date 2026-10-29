use std::collections::HashSet;

pub struct InventoryManager {
    protected_items: HashSet<String>,
    max_slots: usize,
}

impl Default for InventoryManager {
    fn default() -> Self {
        let mut protected = HashSet::new();
        protected.insert("potion_hp".to_string());
        protected.insert("potion_mana".to_string());
        protected.insert("teleport_scroll".to_string());
        Self {
            protected_items: protected,
            max_slots: 60,
        }
    }
}

impl InventoryManager {
    pub fn is_protected(&self, item_id: &str) -> bool {
        self.protected_items.contains(item_id)
    }

    pub fn should_sell_trash(&self, occupied_slots: usize) -> bool {
        (occupied_slots as f64 / self.max_slots as f64) >= 0.80
    }
}
