use anyhow::{anyhow, Result};
use pnet_datalink;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, SocketAddrV4, SocketAddrV6};
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;

// Strong types for the different protocols
pub struct ICMPSocket(Socket);

impl ICMPSocket {
    pub fn new(
        bind_interface: Option<&str>,
        bind_address: Option<IpAddr>,
    ) -> Result<ICMPSocket> {
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
                        None => socket,
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
                        None => socket,
                    }
                }
            },
            None => {
                let socket = Socket::new(
                    Domain::IPV4,
                    Type::RAW,
                    Some(Protocol::ICMPV4),
                )?;
                socket.set_nonblocking(true)?;

                match bind_interface {
                    Some(bi) => bind_to_device(socket, bi)?,
                    None => socket,
                }
            }
        };

        Ok(ICMPSocket(socket))
    }
    pub fn get_mut(&mut self) -> &mut Socket {
        &mut self.0
    }
    pub fn get_ref(&self) -> &Socket {
        &self.0
    }
}

impl AsRawFd for ICMPSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

// Create new ICMP socket, default to IPv4

pub fn new_tcp_socket(
    bind_interface: Option<String>,
    bind_address: IpAddr,
    bind_port: u16,
) -> Result<Socket> {
    let socket = match bind_address {
        IpAddr::V4(address) => {
            let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;

            let socket_address = SocketAddrV4::new(address, bind_port);
            socket.bind(&socket_address.into())?;
            socket.set_nonblocking(true)?;

            socket
        }
        IpAddr::V6(address) => {
            let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;

            // TODO: Figure out what is the correct way to use flowinfo and scope_id of IPv6 Sockets.
            let socket_address = SocketAddrV6::new(address, bind_port, 0, 0);
            socket.bind(&socket_address.into())?;
            socket.set_nonblocking(true)?;

            socket
        }
    };

    let socket = match bind_interface {
        Some(bi) => bind_to_device(socket, &bi)?,
        None => socket,
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
            let error_msg = format!(
                "error binding to device (`{}`): {}",
                bind_interface, err
            );
            Err(std::io::Error::new(std::io::ErrorKind::Other, error_msg))
        } else {
            let error_msg = format!("unexpected error binding device: {}", err);
            Err(std::io::Error::new(std::io::ErrorKind::Other, error_msg))
        };
    }

    Ok(socket)
}

// Get the IP address of the interface in case bind_address is not specified but bind_interface is.
pub fn interface_to_ipaddr(interface: &str) -> Result<IpAddr> {
    let interfaces = pnet_datalink::interfaces();
    let interface = interfaces
        .into_iter()
        .find(|iface| iface.name == interface)
        .ok_or_else(|| anyhow!("interface not found"))?;

    let ipaddr = interface
        .ips
        .into_iter()
        .find(|ip| ip.is_ipv4() || ip.is_ipv6())
        .ok_or_else(|| anyhow!("interface has no IPv4 or IPv6 address"))?;

    Ok(ipaddr.ip())
}
