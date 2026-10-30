// Opens the user's editor from $VISUAL or $EDITOR, offers -q beside --quiet, counts NO_COLOR only when non-empty, and stops on Ctrl-C.
fn main() {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "nano".into());
    let _ = std::process::Command::new(editor).status();
    let quiet = std::env::args().any(|a| a == "--quiet" || a == "-q");
    let colour = !std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let _ = (quiet, colour);
    let _ = ctrlc::set_handler(|| std::process::exit(130));
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
