trait Store {
    fn get(&self, key: &str) -> Option<String>;
}

struct Memory;

impl Store for Memory {
    fn get(&self, _key: &str) -> Option<String> {
        None
    }
}

pub fn lookup(key: &str) -> Option<String> {
    Memory.get(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;

    impl Store for Fake {
        fn get(&self, _key: &str) -> Option<String> {
            Some("fake".into())
        }
    }

    #[test]
    fn a_fake_stands_in() {
        assert_eq!(Fake.get("k").as_deref(), Some("fake"));
    }
}
