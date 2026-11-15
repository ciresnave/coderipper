use tokio::net::TcpStream;

pub async fn dial(addr: &str) -> std::io::Result<TcpStream> {
    TcpStream::connect(addr).await
}
