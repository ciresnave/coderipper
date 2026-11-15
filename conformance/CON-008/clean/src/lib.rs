use std::time::Duration;
use tokio::net::TcpStream;

pub async fn dial(addr: &str) -> std::io::Result<TcpStream> {
    match tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr)).await {
        Ok(connected) => connected,
        Err(_) => Err(std::io::ErrorKind::TimedOut.into()),
    }
}
