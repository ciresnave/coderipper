pub struct Invoice {
    pub id: u64,
    pub total_price_cents: u64,
    pub discount_ratio: f64,
}

pub fn describe(invoice: &Invoice) -> String {
    format!("{}: {}", invoice.id, invoice.total_price_cents)
}
