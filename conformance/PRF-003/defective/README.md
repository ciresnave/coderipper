# defective

`src/lib.rs` calls `b.contains(x)` on a `Vec` declared outside the loop for every `x`; `src/report.rs` rebuilds `s` with `format!("{}{}", s, p)` on every pass.
