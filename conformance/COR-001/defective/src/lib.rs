/// Reads the file.
pub fn read(path: &str) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}
