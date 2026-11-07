fn main() {
    let _ = std::fs::remove_file("cache.tmp");
    if let Ok(text) = std::fs::read_to_string("config.toml") { println!("{text}"); }
}
