use std::{
    fmt,
    net::{IpAddr, SocketAddr, SocketAddrV4, SocketAddrV6},
    os::unix::io::{AsRawFd, RawFd},
};

use anyhow::{anyhow, Result};
use pnet_datalink;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
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

// Create new TCP socket

pub struct TCPSocket(Socket);

impl TCPSocket {
    pub fn new(
        bind_interface: Option<&str>,
        bind_address: Option<(IpAddr, u16)>,
    ) -> Result<TCPSocket> {
        // Check bind_addr is an IPv4 or IPv6 address
        let socket = match bind_address {
            Some(addr) => match addr.0 {
                IpAddr::V4(_) => {
                    let socket = Socket::new(
                        Domain::IPV4,
                        Type::STREAM,
                        Some(Protocol::TCP),
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
                        Type::STREAM,
                        Some(Protocol::TCP),
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
                    Type::STREAM,
                    Some(Protocol::TCP),
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

        Ok(TCPSocket(socket))
    }
    pub fn get_mut(&mut self) -> &mut Socket {
        &mut self.0
    }
    pub fn get_ref(&self) -> &Socket {
        &self.0
    }
    pub fn connect(&mut self, addr: SocketAddr) -> Result<()> {
        self.0.set_nonblocking(false)?; // Set to blocking as we are waiting for a connection
                                        // otherwise we get a WouldBlock error
        self.0.connect(&addr.into())?;
        self.0.set_nonblocking(true)?;
        Ok(())
    }
    pub fn listen(&mut self, backlog: i32) -> Result<()> {
        self.0.listen(backlog)?;
        Ok(())
    }
}

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
/// Init python venv for integration tests using dummynet.
pub fn init_venv() {
    create_venv();
    install_deps();
}
/// Run integration tests in integration_test directory relative to the current
/// binary under test
pub fn run_pytest() {
    let mut cmd = std::process::Command::new(format!("venv/bin/python"));
    cmd.arg("-m").arg("pytest").arg("integration_test/");
    let output = cmd.output().expect("Failed to run test");
    // For printing the output of the of the pytest tests
    println!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(output.status.success());
}
/// delete the venv directory for integration tests
pub fn delete_venv() {
    std::process::Command::new("rm")
        .arg("-rf")
        .arg("venv")
        .output()
        .expect("Failed to delete venv");
}

// Create a virtual environment
fn create_venv() {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("-m").arg("venv").arg("venv");
    let output = cmd.output().expect("Failed to create venv");
    assert!(output.status.success());
}
fn install_deps() {
    let mut cmd = std::process::Command::new(format!("venv/bin/pip"));
    cmd.arg("install")
        .arg("-r")
        .arg(format!("integration_test/requirements.in"));
    let output = cmd.output().expect("Failed to install deps");
    assert!(output.status.success());
}

pub struct Statistics {
    mean: f64,
    variance: f64,
    standard_deviation: f64,
    min: f64,
    max: f64,
    samples: usize,
}

impl fmt::Display for Statistics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "mean: {:.2} variance: {:.2} standard deviation: {:.2} min: {:.2} \
             max: {:.2} samples: {}",
            self.mean(),
            self.variance(),
            self.standard_deviation(),
            self.min(),
            self.max(),
            self.samples()
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn stats_test() {
        let mut stats = Statistics::new();
        stats.update(1.0);
        stats.update(2.0);
        stats.update(3.0);
        stats.update(4.0);
        stats.update(5.0);
        stats.update(6.0);
        stats.update(7.0);
        stats.update(8.0);
        stats.update(9.0);
        stats.update(10.0);

        assert_eq!(stats.mean(), 5.5);
        assert_eq!(stats.variance(), 8.25);
        assert_eq!(stats.standard_deviation().round(), 3.0);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 10.0);
        assert_eq!(stats.samples, 10);
    }
}

impl Statistics {
    pub fn new() -> Self {
        Self {
            mean: f64::NAN,
            variance: f64::NAN,
            standard_deviation: f64::NAN,
            min: f64::NAN,
            max: f64::NAN,
            samples: 0,
        }
    }

    pub fn mean(&self) -> f64 {
        self.mean
    }
    pub fn variance(&self) -> f64 {
        self.variance / ((self.samples) as f64)
    }
    pub fn standard_deviation(&self) -> f64 {
        self.variance().sqrt()
    }

    pub fn min(&self) -> f64 {
        self.min
    }

    pub fn max(&self) -> f64 {
        self.max
    }
    pub fn samples(&self) -> usize {
        self.samples
    }

    pub fn update(&mut self, value: f64) {
        self.samples += 1;
        if self.samples == 1 {
            self.mean = value;
            self.variance = 0.0;
            self.standard_deviation = 0.0;
            self.min = value;
            self.max = value;
        } else {
            let old_mean = self.mean;
            self.mean = old_mean + (value - old_mean) / self.samples as f64;
            self.variance =
                self.variance + (value - old_mean) * (value - self.mean);
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }
    }
}
