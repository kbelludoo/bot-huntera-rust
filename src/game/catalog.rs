#[derive(Debug, Clone)]
pub struct AreaCandidate {
    pub id: &'static str,
    pub min_level: u32,
    pub max_level: u32,
    pub base_exp: f64,
    pub base_gold: f64,
}

pub static HUNTS_CATALOG: &[AreaCandidate] = &[
        AreaCandidate { id: "sewer_rats", min_level: 1, max_level: 15, base_exp: 90.0, base_gold: 45.0 },
        AreaCandidate { id: "troll_caves", min_level: 15, max_level: 35, base_exp: 220.0, base_gold: 110.0 },
        AreaCandidate { id: "rotworms_valley", min_level: 35, max_level: 60, base_exp: 550.0, base_gold: 320.0 },
        AreaCandidate { id: "cyclops_camp", min_level: 60, max_level: 90, base_exp: 1400.0, base_gold: 850.0 },
        AreaCandidate { id: "dragon_lair", min_level: 90, max_level: 140, base_exp: 3600.0, base_gold: 2200.0 },
        AreaCandidate { id: "giant_spider_tomb", min_level: 140, max_level: 200, base_exp: 8800.0, base_gold: 5500.0 },
        AreaCandidate { id: "demon_sanctum", min_level: 200, max_level: 350, base_exp: 24000.0, base_gold: 16000.0 },
];

#[derive(Clone, Default)]
pub struct Catalog;

impl Catalog {
    pub fn select_best_hunt(&self, level: u32) -> &'static AreaCandidate {
        for area in HUNTS_CATALOG {
            if level >= area.min_level && level <= area.max_level {
                return area;
            }
        }
        &HUNTS_CATALOG[HUNTS_CATALOG.len() - 1]
    }
}
