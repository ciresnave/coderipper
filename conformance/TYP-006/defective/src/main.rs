fn main() {
    let port: u16 = std::env::args().nth(1).unwrap().parse().expect("number");
    println!("{port}");
}
