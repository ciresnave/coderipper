trait Store {
    fn get(&self, k: u8) -> u8;
}

struct Disk;
struct Network;

impl Store for Disk {
    fn get(&self, k: u8) -> u8 {
        k
    }
}

impl Store for Network {
    fn get(&self, k: u8) -> u8 {
        k + 1
    }
}

pub fn read(k: u8) -> u8 {
    Disk.get(k) + Network.get(k)
}
