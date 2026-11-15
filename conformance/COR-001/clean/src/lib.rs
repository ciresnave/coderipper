/// Reads the file.
///
/// # Errors
///
/// Returns the I/O error when the file cannot be read.
pub fn read(path: &str) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}
