//! The composition root: the one place the adapter is built.
use store::DiskStore;

fn main() {
    let s = DiskStore::new(1);
    println!("{}", core::load(&s, 2));
}
