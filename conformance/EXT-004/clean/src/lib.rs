pub trait Reader {
    fn get(&self);
    fn list(&self);
}

pub trait Writer {
    fn put(&self);
    fn delete(&self);
}

pub struct ReadOnly;

impl Reader for ReadOnly {
    fn get(&self) {}
    fn list(&self) {}
}

/// Six methods, but the implementer gives each of them a body, and the one it rarely needs has a default.
pub trait Store {
    fn get(&self);
    fn put(&self);
    fn delete(&self);
    fn list(&self);
    fn flush(&self);
    fn close(&self) {}
}

pub struct Memory;

impl Store for Memory {
    fn get(&self) {
        println!("get");
    }
    fn put(&self) {
        println!("put");
    }
    fn delete(&self) {
        println!("delete");
    }
    fn list(&self) {
        println!("list");
    }
    fn flush(&self) {
        println!("flush");
    }
}
