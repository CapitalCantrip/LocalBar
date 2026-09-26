use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use socket2::{Domain, Protocol, Socket, Type};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

/// Returns true when a process is already listening on `(host, port)`.
///
/// For loopback / unspecified addresses probes both 127.0.0.1 and ::1 with a
/// bind probe (no network flow, immune to macOS network-extension latency).
/// For remote hosts falls back to a TCP connect probe with a 1 s timeout.
pub fn port_is_open(host: &str, port: u16) -> bool {
    let Ok(mut addrs) = (host, port).to_socket_addrs() else { return false };
    let Some(addr) = addrs.next() else { return false };
    if is_local(&addr) { loopback_bind_probes(port) } else { connect_probe(addr) }
}

fn is_local(addr: &SocketAddr) -> bool {
    match addr.ip() {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_unspecified(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unspecified(),
    }
}

// Probes both IPv4 and IPv6 loopback so we catch a holder regardless of which
// family the server bound to, even when the caller resolves "localhost" to only
// one family.
fn loopback_bind_probes(port: u16) -> bool {
    let v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port);
    bind_probe(v4) || bind_probe(v6)
}

fn bind_probe(addr: SocketAddr) -> bool {
    let domain = if addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
    let Ok(socket) = Socket::new(domain, Type::STREAM, Some(Protocol::TCP)) else {
        return connect_probe(addr);
    };
    if socket.set_reuse_address(false).is_err() {
        return connect_probe(addr);
    }
    match socket.bind(&addr.into()) {
        Ok(()) => false,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => true,
        Err(_) => connect_probe(addr),
    }
}

fn connect_probe(addr: SocketAddr) -> bool {
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok()
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::port_is_open;

    #[test]
    fn loopback_listener_detected_as_open() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port_is_open("127.0.0.1", port));
    }

    #[test]
    fn wildcard_listener_detected_via_loopback_probe() {
        let listener = TcpListener::bind("0.0.0.0:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port_is_open("127.0.0.1", port));
    }

    #[test]
    fn localhost_string_detected_as_open() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port_is_open("localhost", port));
    }

    #[test]
    fn free_port_detected_as_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(!port_is_open("127.0.0.1", port));
    }
}
