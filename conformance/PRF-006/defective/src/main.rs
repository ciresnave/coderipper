fn sum(xs: &[u32]) -> u32 {
    // PERF-EXCEPTION: get_unchecked is faster
    let mut t = 0;
    for i in 0..xs.len() {
        t += unsafe { *xs.get_unchecked(i) };
    }
    t
}

fn main() {
    println!("{}", sum(&[1, 2, 3]));
}
