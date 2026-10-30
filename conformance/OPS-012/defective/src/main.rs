// The seeded defects: a fixed editor, a long flag without its short form, a NO_COLOR test that counts an empty value, and a loop nothing can stop.
fn main() {
    let _ = std::process::Command::new("nano").status();
    let quiet = std::env::args().any(|a| a == "--quiet");
    let colour = std::env::var_os("NO_COLOR").is_some();
    let _ = (quiet, colour);
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
