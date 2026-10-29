fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V") => println!("fixture {}", env!("CARGO_PKG_VERSION")),
        Some("--help" | "-h") => println!("usage: fixture [--version] [--help]"),
        Some(other) => {
            eprintln!("error: unknown flag `{other}`");
            std::process::exit(2);
        }
        None => println!("fixture: nothing to do"),
    }
}
