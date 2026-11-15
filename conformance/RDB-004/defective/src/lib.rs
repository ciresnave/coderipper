pub fn eight(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8) -> u8 {
    a + b + c + d + e + f + g + h
}

pub fn five_deep(x: u8) -> u8 {
    if x > 0 {
        if x > 1 {
            if x > 2 {
                if x > 3 {
                    if x > 4 {
                        return x;
                    }
                }
            }
        }
    }
    0
}

pub fn switches(go: bool) -> u8 {
    if go {
        1
    } else {
        2
    }
}
