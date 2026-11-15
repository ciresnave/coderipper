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
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn decoding_an_encoding_gives_back_the_input(input in proptest::collection::vec(any::<u8>(), 0..64)) {
            prop_assert_eq!(Doubler::decode(&Doubler::encode(&input)), input);
        }
    }
}
