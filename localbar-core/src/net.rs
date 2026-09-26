use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use socket2::{Domain, Protocol, Socket, Type};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

pub fn port_is_open(host: &str, port: u16) -> bool {
    let Ok(mut addrs) = (host, port).to_socket_addrs() else { return false };
    let Some(addr) = addrs.next() else { return false };
    if is_local(&addr) { probe_both_loopback_families(port) } else { connect_probe(addr) }
}

fn is_local(addr: &SocketAddr) -> bool {
    match addr.ip() {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_unspecified(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unspecified(),
    }
}

fn probe_both_loopback_families(port: u16) -> bool {
    [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    ]
    .into_iter()
    .any(|ip| bind_probe(SocketAddr::new(ip, port)))
}

fn bind_probe(addr: SocketAddr) -> bool {
    let domain = if addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
    let Ok(socket) = Socket::new(domain, Type::STREAM, Some(Protocol::TCP)) else {
        return connect_probe(addr);
    };
    if socket.set_reuse_address(true).is_err() {
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
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};

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

    #[test]
    fn ipv6_loopback_listener_detected_as_open() {
        let Ok(listener) = TcpListener::bind("[::1]:0") else { return };
        let port = listener.local_addr().unwrap().port();
        assert!(port_is_open("127.0.0.1", port));
    }

    #[test]
    fn time_wait_after_server_exit_is_not_a_conflict() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (server_side, _) = listener.accept().unwrap();
        drop(server_side);
        let _ = client.read(&mut [0u8; 1]);
        drop(client);
        drop(listener);
        assert!(!port_is_open("127.0.0.1", port));
    }
}
