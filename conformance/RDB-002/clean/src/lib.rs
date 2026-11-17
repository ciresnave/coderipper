//! The clean twin: the same function in snake case, and a legacy name kept on purpose under an allow that names the lint.
pub fn compute_total(prices: &[u32]) -> u32 {
    prices.iter().sum()
}

#[allow(non_snake_case)]
pub fn LegacyEntry() {}
