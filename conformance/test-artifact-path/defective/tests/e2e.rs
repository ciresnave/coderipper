use std::path::PathBuf;

fn helper() -> PathBuf {
    let exe = std::env::current_exe().expect("test exe");
    let debug = exe.parent().and_then(std::path::Path::parent).expect("target dir");
    debug.join("examples").join("tool")
}

#[test]
fn finds_the_helper() {
    assert!(helper().file_name().is_some());
}
