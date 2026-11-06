pub trait Store {
    fn get(&self, key: &str) -> Option<String>;
    fn delete(&mut self, key: &str);
}

pub struct Memory;

impl Store for Memory {
    fn get(&self, _key: &str) -> Option<String> {
        None
    }

    fn delete(&mut self, _key: &str) {
        todo!()
    }
}
