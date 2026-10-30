// The seeded defect: a fixed editor that ignores the user's.
fn main() {
    let _ = std::process::Command::new("nano").status();
}
