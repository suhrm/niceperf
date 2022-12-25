use anyhow::Result;
use socket2::{Domain, Protocol, Socket, Type};
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

                match bind_interface {
                    Some(bi) => bind_to_device(socket, bi)?,
                    None => socket
                }
            }

            IpAddr::V6(_) => {
                let socket = Socket::new(
                    Domain::IPV6,
                    Type::RAW,
                    Some(Protocol::ICMPV6),
                )?;
                socket.set_nonblocking(true)?;

                match bind_interface {
                    Some(bi) => bind_to_device(socket, bi)?,
                    None => socket
                }
            }
        },
        None => {
            let socket =
                Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?;
            socket.set_nonblocking(true)?;

            match bind_interface {
                Some(bi) => bind_to_device(socket, bi)?,
                None => socket
            }
        }
    };

    Ok(socket)
}


pub fn bind_to_device(
    socket: Socket,
    bind_interface: &str,
) -> Result<Socket, std::io::Error> {
    // Socket2 bind_device does not have nice error types, so we have to handle the libc errors.
    // In case, we get an error when binding, map it into a more friendly std::io::Error
    if let Err(err) = socket.bind_device(Some(bind_interface.as_bytes())) {
        return if matches!(err.raw_os_error(), Some(libc::ENODEV)) {
            let error_msg = format!("error binding to device (`{}`): {}", bind_interface, err);
            Err(std::io::Error::new(std::io::ErrorKind::Other, error_msg))
        } else {
            let error_msg = format!("unexpected error binding device: {}", err);
            Err(std::io::Error::new(std::io::ErrorKind::Other, error_msg))
        }
    }

    Ok(socket)
}