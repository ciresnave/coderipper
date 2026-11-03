pub struct Money {
    pub cents: i64,
}

impl Money {
    pub fn new(cents: i64) -> Result<Self, String> {
        if cents < 0 {
            return Err("negative".into());
        }
        Ok(Self { cents })
    }
}
