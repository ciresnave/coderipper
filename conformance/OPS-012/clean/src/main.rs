// Opens the user's editor from $VISUAL or $EDITOR.
fn main() {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "nano".into());
    let _ = std::process::Command::new(editor).status();
}
