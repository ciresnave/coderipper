/// Reads the length field of a header.
pub fn parse_header(input: &str) -> usize {
    input.split(':').next().map_or(0, str::len)
}
