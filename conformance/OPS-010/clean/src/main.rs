fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("tool {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    println!("running");
}
