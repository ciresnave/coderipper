fn main() {
    println!("{}", fixture::used());
    fixture::with(|| println!("scoped"));
}
