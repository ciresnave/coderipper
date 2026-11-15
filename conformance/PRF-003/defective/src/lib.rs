pub mod report;

pub fn shared(a: &[u32], b: Vec<u32>) -> usize {
    let mut n = 0;
    for x in a {
        if b.contains(x) {
            n += 1;
        }
    }
    n
}
