# defective

`src/lib.rs` keeps a global `CACHE` map that `remember` inserts into and nothing ever removes from; `src/store.rs` pushes onto a `response_cache` field and never trims it. The plain `items` vector in the same struct is not a candidate.
