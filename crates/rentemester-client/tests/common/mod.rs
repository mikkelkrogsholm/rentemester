//! Loopback fixture transport: answer passive discovery independently of the
//! existing exact business request/body/count assertions.
#![allow(dead_code)]
use std::io::{Read, Write};

fn discovery(stream: &mut (impl Read + Write)) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        bytes.push(byte[0]);
        assert!(bytes.len() < 16384);
    }
    let wire = String::from_utf8(bytes).unwrap().to_lowercase();
    assert!(wire.starts_with("get /api/protocol http/1.1\r\n"));
    for forbidden in [
        "cookie:",
        "authorization:",
        "x-rentemester-service-key:",
        "x-rentemester-protocol:",
    ] {
        assert!(
            !wire.contains(forbidden),
            "credential/gate leaked to discovery"
        );
    }
    let body = include_str!("manifest.json");
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

pub struct TcpListener(std::net::TcpListener);
impl TcpListener {
    pub fn bind(addr: impl std::net::ToSocketAddrs) -> std::io::Result<Self> {
        std::net::TcpListener::bind(addr).map(Self)
    }
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.0.local_addr()
    }
    pub fn set_nonblocking(&self, value: bool) -> std::io::Result<()> {
        self.0.set_nonblocking(value)
    }
    pub fn accept(&self) -> std::io::Result<(std::net::TcpStream, std::net::SocketAddr)> {
        loop {
            let (mut stream, addr) = self.0.accept()?;
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
            let mut peek = [0; 64];
            let n = stream.peek(&mut peek)?;
            if peek[..n].starts_with(b"GET /api/protocol ") {
                discovery(&mut stream)?;
            } else {
                return Ok((stream, addr));
            }
        }
    }
    pub fn incoming(&self) -> impl Iterator<Item = std::io::Result<std::net::TcpStream>> + '_ {
        std::iter::from_fn(|| Some(self.accept().map(|(stream, _)| stream)))
    }
}

#[cfg(unix)]
pub fn unix_accept(
    listener: &std::os::unix::net::UnixListener,
) -> std::io::Result<(
    std::os::unix::net::UnixStream,
    std::os::unix::net::SocketAddr,
)> {
    let (mut stream, _) = listener.accept()?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    discovery(&mut stream)?;
    listener.accept()
}
