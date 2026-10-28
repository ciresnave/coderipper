use std::sync::Mutex;

pub struct S {
    a: Mutex<i32>,
    b: Mutex<i32>,
}

impl S {
    pub fn one(&self) {
        let _a = self.a.lock().unwrap();
        let _b = self.b.lock().unwrap();
    }

    pub fn two(&self) {
        let _a = self.a.lock().unwrap();
        let _b = self.b.lock().unwrap();
    }
}
