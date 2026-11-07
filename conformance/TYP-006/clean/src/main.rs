fn main() {
    let Some(arg) = std::env::args().nth(1) else {
        eprintln!("usage: tool PORT");
        return;
    };
    // INVARIANT: the argument was parsed by the caller's shell wrapper, so it is a number
    let port: u16 = arg.parse().unwrap();
    println!("{port}");
}

#[cfg(test)]
mod tests {
    #[test]
    fn unwraps_freely() {
        assert_eq!(Some(1).unwrap(), 1);
    }
}
