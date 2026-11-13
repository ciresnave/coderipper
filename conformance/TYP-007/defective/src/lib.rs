pub fn open(path: &str, create: bool, truncate: bool) -> usize {
    path.len() + usize::from(create) + usize::from(truncate)
}
