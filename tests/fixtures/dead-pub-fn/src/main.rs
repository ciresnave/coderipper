// Binary crate, not a lib -- `fn main()` is a genuine reachability root regardless of visibility,
// which a pure lib crate (with everything downgraded to pub(crate) by the rewrite) does not have.
// See the plan's ledger, Task 6 ruling, for why the original lib+`pub use` design doesn't work.

pub fn dead_function() -> i32 {
    42
}

pub fn used_function() -> i32 {
    1
}

pub fn caller() -> i32 {
    used_function()
}

fn main() {
    println!("{}", caller());
}
