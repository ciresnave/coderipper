use std::sync::Mutex;

pub struct Cache {
    entries: Mutex<Vec<String>>,
}

impl Cache {
    pub async fn refresh(&self) {
        let mut entries = self.entries.lock().unwrap();
        let fresh = fetch().await;
        entries.push(fresh);
    }
}

async fn fetch() -> String {
    String::new()
}
