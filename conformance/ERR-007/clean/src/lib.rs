pub fn send_one(open: fn() -> Result<Conn, ()>) -> Result<(), ()> {
    let conn = open()?;
    let sent = conn.send(1);
    conn.close();
    sent
}

pub struct Conn;
impl Conn {
    pub fn send(&self, _: u8) -> Result<(), ()> {
        Ok(())
    }
    pub fn close(&self) {}
}
