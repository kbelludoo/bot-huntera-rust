use crate::game::catalog::{AreaCandidate, Catalog};

pub struct RouteSolver {
    catalog: Catalog,
}

impl Default for RouteSolver {
    fn default() -> Self {
        Self {
            catalog: Catalog::default(),
        }
    }
}

impl RouteSolver {
    pub fn new(catalog: Catalog) -> Self {
        Self { catalog }
    }

    pub fn select_best_hunt(&self, level: u32) -> &'static AreaCandidate {
        self.catalog.select_best_hunt(level)
    }
}
