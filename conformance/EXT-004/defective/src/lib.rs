pub trait Store {
    fn get(&self);
    fn put(&self);
    fn delete(&self);
    fn list(&self);
    fn flush(&self);
    fn close(&self);
}

pub struct ReadOnly;

impl Store for ReadOnly {
    fn get(&self) {
        println!("get");
    }
    fn put(&self) {
        unimplemented!()
    }
    fn delete(&self) {
        todo!()
    }
    fn list(&self) {}
    fn flush(&self) {}
    fn close(&self) {}
}
