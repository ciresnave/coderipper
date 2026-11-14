pub enum Create {
    IfMissing,
    Never,
}

pub enum Truncate {
    Yes,
    No,
}

pub fn open(path: &str, create: Create, truncate: Truncate) -> usize {
    path.len() + usize::from(matches!(create, Create::IfMissing)) + usize::from(matches!(truncate, Truncate::Yes))
}
