trait Store {
    fn get(&self, k: u8) -> u8;
}

struct Disk;

impl Store for Disk {
    fn get(&self, k: u8) -> u8 {
        k
    }
}

pub fn read(k: u8) -> u8 {
    Disk.get(k)
}
