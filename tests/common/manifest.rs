//! The minimal `Cargo.toml` fixture text. Its own file so that only the tests that use it compile it.

pub const MANIFEST: &str =
    "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
