use std::process::Command;

fn tool() -> &'static str {
    env!("CARGO_BIN_EXE_fixture")
}

#[test]
fn finds_the_binary_through_cargo() {
    assert!(!tool().is_empty());
}

/// Running the test binary itself again derives no sibling path.
#[test]
fn child() {
    let _ = Command::new(std::env::current_exe().unwrap()).arg("--list").output();
}
