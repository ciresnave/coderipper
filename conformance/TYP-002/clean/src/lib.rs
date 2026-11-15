use std::str::FromStr;

pub enum Kind {
    Low,
    Mid,
    High,
}

impl FromStr for Kind {
    type Err = ();

    /// Outside text becomes a typed value once, here.
    fn from_str(s: &str) -> Result<Kind, ()> {
        match s {
            "low" => Ok(Kind::Low),
            "mid" => Ok(Kind::Mid),
            "high" => Ok(Kind::High),
            _ => Err(()),
        }
    }
}

pub fn rank(kind: Kind) -> u8 {
    match kind {
        Kind::Low => 1,
        Kind::Mid => 2,
        Kind::High => 3,
    }
}

pub fn short(s: &str) -> u8 {
    match s {
        "a" => 1,
        "b" => 2,
        _ => 0,
    }
}
