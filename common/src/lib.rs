use anyhow::Result;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::net::IpAddr;


// Create new ICMP socket, default to IPv4
pub fn new_icmp_socket(
    bind_interface: Option<&str>,
    bind_address: Option<IpAddr>,
) -> Result<Socket> {
    // Check bind_addr is an IPv4 or IPv6 address
    let socket = match bind_address {
        Some(addr) => match addr {
            IpAddr::V4(_) => {
                let socket = Socket::new(
                    Domain::IPV4,
                    Type::RAW,
                    Some(Protocol::ICMPV4),
                )?;
                socket.set_nonblocking(true)?;
                socket
            }

            IpAddr::V6(_) => {
                let socket = Socket::new(
                    Domain::IPV6,
                    Type::RAW,
                    Some(Protocol::ICMPV6),
                )?;
                socket.set_nonblocking(true)?;
                socket
            }
        },
        None => {
            let socket =
                Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?;
            socket.set_nonblocking(true)?;
            socket
        }
    };

    Ok(socket)
}
