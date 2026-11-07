fn main() {
    // the cache file may already be gone, and nothing depends on it
    let _ = std::fs::remove_file("cache.tmp");
    match std::fs::read_to_string("config.toml") {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("cannot read config.toml: {e}"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn drops_freely() {
        let _ = std::fs::remove_file("x");
    }
}
