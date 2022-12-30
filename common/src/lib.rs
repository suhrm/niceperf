use std::{
    net::{IpAddr, SocketAddr, SocketAddrV4, SocketAddrV6},
    os::unix::io::{AsRawFd, RawFd},
};

use anyhow::{anyhow, Result};
use pnet_datalink;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::io::unix::AsyncFd;

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
pub struct AsyncICMPSocket {
    inner: AsyncFd<ICMPSocket>,
}

impl AsyncICMPSocket {
    pub fn new(socket: ICMPSocket) -> Result<Self> {
        Ok(Self {
            inner: AsyncFd::new(socket)?,
        })
    }

    pub async fn send_to(
        &mut self,
        packet: &[u8],
        addr: &IpAddr,
    ) -> Result<(usize)> {
        let mut guard = self.inner.writable().await?;
        let addr = match addr {
            IpAddr::V4(addr) => {
                let addr = std::net::SocketAddr::V4(
                    std::net::SocketAddrV4::new(*addr, 0),
                );
                socket2::SockAddr::from(addr)
            }
            IpAddr::V6(addr) => {
                // TODO : Check if this is correct
                let addr = std::net::SocketAddr::V6(
                    std::net::SocketAddrV6::new(*addr, 0, 0, 0),
                );
                socket2::SockAddr::from(addr)
            }
        };
        match guard
            .try_io(|inner| inner.get_ref().get_ref().send_to(packet, &addr))
        {
            Ok(res) => Ok(res?),
            Err(e) => Err(anyhow!("Error sending packet")),
        }
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            let mut guard = self.inner.readable().await?;
            // Safety: We are sure that the buffer is initialized
            let uninit_slice = unsafe { core::mem::transmute(&mut *buf) };

            match guard
                .try_io(|inner| inner.get_ref().get_ref().recv(uninit_slice))
            {
                Ok(Ok(n)) => return Ok(n),
                Ok(Err(e)) => Err(anyhow!(e.to_string()))?,
                Err(_would_block) => continue,
            }
        }
    }
}

// UDP strongly typed socket
pub struct UDPSocket(Socket);

impl UDPSocket {
    pub fn new(
        bind_interface: Option<&str>,
        bind_address: Option<(IpAddr, u16)>,
    ) -> Result<UDPSocket> {
        // Check bind_addr is an IPv4 or IPv6 address
        let socket = match bind_address {
            Some(addr) => match addr.0 {
                IpAddr::V4(_) => {
                    let socket = Socket::new(
                        Domain::IPV4,
                        Type::DGRAM,
                        Some(Protocol::UDP),
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
                        Type::DGRAM,
                        Some(Protocol::UDP),
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
                    Type::DGRAM,
                    Some(Protocol::UDP),
                )?;
                socket.set_nonblocking(true)?;

                match bind_interface {
                    Some(bi) => bind_to_device(socket, bi)?,
                    None => socket,
                }
            }
        };
        match bind_address {
            // Bind to the address and an ephemeral port
            Some((addr, 0)) => {
                let socket_address = SocketAddr::new(addr, 0);
                socket.bind(&socket_address.into())?;
            }
            // Bind to provided port and address
            Some((addr, port)) => {
                let socket_address = SocketAddr::new(addr, port);
                socket.bind(&socket_address.into())?;
            }
            // Otherwise request an ephemeral port
            None => {}
        }

        Ok(UDPSocket(socket))
    }
    pub fn get_mut(&mut self) -> &mut Socket {
        &mut self.0
    }
    pub fn get_ref(&self) -> &Socket {
        &self.0
    }
}

// Create new ICMP socket, default to IPv4

pub fn new_tcp_socket(
    bind_interface: Option<String>,
    bind_address: IpAddr,
    bind_port: Option<u16>,
) -> Result<Socket> {
    let socket = match bind_address {
        IpAddr::V4(..) => {
            let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;
            socket.set_nonblocking(true)?;

            socket
        }
        IpAddr::V6(..) => {
            let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;
            socket.set_nonblocking(true)?;

            socket
        }
    };

    let socket = match bind_interface {
        Some(bi) => bind_to_device(socket, &bi)?,
        None => socket,
    };

    match bind_port {
        // Bind to provided port
        Some(port) => {
            let socket_address = SocketAddr::new(bind_address, port);
            socket.bind(&socket_address.into())?;
        }
        // Otherwise request an ephemeral port
        None => {
            let socket_address = SocketAddr::new(bind_address, 0);
            socket.bind(&socket_address.into())?;
        }
    }

    Ok(socket)
}

pub fn bind_to_device(
    socket: Socket,
    bind_interface: &str,
) -> Result<Socket, std::io::Error> {
    // Socket2 bind_device does not have nice error types, so we have to handle
    // the libc errors. In case, we get an error when binding, map it into a
    // more friendly std::io::Error
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

// Get the IP address of the interface in case bind_address is not specified but
// bind_interface is.
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
