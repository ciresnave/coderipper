pub mod report;

use std::collections::HashSet;

pub fn shared(a: &[u32], b: Vec<u32>) -> usize {
    let lookup: HashSet<u32> = b.into_iter().collect();
    let mut n = 0;
    for x in a {
        if lookup.contains(x) {
            n += 1;
        }
    }
    n
}

pub fn rows(rows: &[Vec<u32>]) -> usize {
    let mut n = 0;
    for row in rows {
        let local = vec![1, 2, 3];
        if row.contains(&1) || local.contains(&2) {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_scan_in_a_test_is_free() {
        let b = vec![1u32, 2, 3];
        for x in 0..3u32 {
            assert!(b.contains(&(x + 1)));
        }
    }
}
