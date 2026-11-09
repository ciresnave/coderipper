pub struct Options {
    pub a: u8,
    pub b: u8,
}

pub fn seven(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8) -> u8 {
    a + b + c + d + e + f + g
}

pub fn four_deep(x: u8) -> u8 {
    if x > 0 {
        if x > 1 {
            if x > 2 {
                if x > 3 {
                    return x;
                }
            }
        }
    }
    0
}

pub fn passes_it_on(go: bool) -> u8 {
    other(go)
}

pub fn combined(go: bool, x: u8) -> u8 {
    if go && x > 0 {
        1
    } else {
        2
    }
}

fn other(go: bool) -> u8 {
    u8::from(go)
}

pub trait Sink {
    fn put(&mut self, a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8);
}

pub struct Null;

impl Sink for Null {
    fn put(&mut self, _a: u8, _b: u8, _c: u8, _d: u8, _e: u8, _f: u8, _g: u8, _h: u8) {}
}

#[cfg(test)]
mod tests {
    fn flagged(go: bool) -> u8 {
        if go {
            1
        } else {
            2
        }
    }

    fn many(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8) -> u8 {
        a + b + c + d + e + f + g + h
    }

    #[test]
    fn shapes_in_tests_are_free() {
        assert_eq!(flagged(true), 1);
        assert_eq!(many(1, 1, 1, 1, 1, 1, 1, 1), 8);
    }
}
