pub fn classify(a: bool, b: bool, c: bool, d: bool, e: bool, f: bool) -> u32 {
    if a {
        if b {
            if c {
                if d {
                    if e {
                        if f {
                            return 1;
                        }
                    }
                }
            }
        }
    }
    0
}
