/// A codec that stores each byte twice.
pub struct Doubler;

impl Doubler {
    pub fn encode(input: &[u8]) -> Vec<u8> {
        input.iter().flat_map(|&b| [b, b]).collect()
    }

    pub fn decode(input: &[u8]) -> Vec<u8> {
        input.chunks(2).map(|c| c[0]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_example() {
        assert_eq!(Doubler::decode(&Doubler::encode(b"ab")), b"ab");
    }
}
