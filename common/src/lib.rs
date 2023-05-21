use std::{
    ffi::{OsStr, OsString},
    fmt, fs,
    net::{IpAddr, SocketAddr, SocketAddrV4, SocketAddrV6},
    os::unix::io::{AsRawFd, RawFd},
    sync::Arc,
};

use anyhow::{anyhow, Result};
use pnet_datalink;
use quinn::{ClientConfig, ServerConfig, VarInt};
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
    ) -> Result<usize> {
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
            Err(_e) => Err(anyhow!("Error sending packet")),
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
        maximum_segment_size: Option<u16>,
        congestion_control: Option<String>,
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

        // Set the maximum segment size
        match maximum_segment_size {
            Some(mss) => {
                socket.set_mss(mss.into())?;
            }
            None => {}
        }
        // Set TCP_NODELAY
        socket.set_nodelay(true)?;

        // Set the congestion control algorithm if provided
        // otherwise use the default OS algorithm
        if let Some(cc) = congestion_control {
            nix::sys::socket::setsockopt(
                socket.as_raw_fd(),
                nix::sys::socket::sockopt::TcpCongestion,
                &OsString::from(cc),
            )
            .map_err(|e| {
                anyhow!(
                    "Failed to set congestion control algorithm: {}",
                    e.to_string()
                )
                // TODO: get available congestion control algorithms from the OS
                // and print them
            })?;
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

pub struct QuicClient {
    pub client: quinn::Endpoint, // quinn client
    pub socket: UDPSocket,       // Underlying socket for the quic connection
}

pub struct QuicServer {
    pub server: quinn::Endpoint,
    pub socket: UDPSocket, // Underlying socket for the quic connection
}

impl QuicClient {
    pub fn new(
        bind_interface: Option<&str>, // Interface to bind to
        bind_address: Option<(IpAddr, u16)>, // Address to bind to
        cert_path: Option<&str>,      // Path to the certificate
    ) -> Result<Self> {
        let socket = UDPSocket::new(None, None)?; // We do not bind to a
                                                  // Create a quinn client to a specific address
        let std_sock = std::net::UdpSocket::from(
            socket.get_ref().try_clone()?.try_clone()?,
        );
        // TODO: This is a bit hacky, but it works for now.

        let client_config = configure_client(cert_path)?;
        // Is this really needed since we rebind the socket later? maybe for the
        // control channel?
        let mut client = match bind_address {
            Some(addr) => quinn::Endpoint::client(addr.into())?,
            None => quinn::Endpoint::client(
                ("0.0.0.0".parse::<IpAddr>()?, 0).into(),
            )?,
        };

        client.set_default_client_config(client_config);

        client.rebind(std_sock)?;
        Ok(Self { client, socket })
    }
    pub async fn connect(
        &self,
        server_addr: (IpAddr, u16),
    ) -> Result<quinn::Connection> {
        self.client
            .connect(server_addr.into(), "localhost")?
            .await
            .map_err(|e| e.into())
    }
}

impl QuicServer {
    pub fn new(addr: (IpAddr, u16)) -> Result<Self> {
        let socket = UDPSocket::new(None, None)?; // We do not bind to a
                                                  // Create a quinn client to a specific address
        let std_sock = std::net::UdpSocket::from(
            socket.get_ref().try_clone()?.try_clone()?,
        );

        let (server_config, _server_cert) = configure_server()?;

        let server = quinn::Endpoint::server(server_config, addr.into())?;

        Ok(Self { server, socket })
    }
}
/// Dummy certificate verifier that treats any certificate as valid.
/// NOTE, such verification is vulnerable to MITM attacks, but convenient for
/// testing.
struct SkipServerVerification;

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}
fn configure_client(cert_path: Option<&str>) -> Result<ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();

    let cert_path = cert_path.unwrap_or("cert.der");
    let cert_path = std::env::current_dir()?.join(cert_path);
    dbg!(&cert_path);

    fs::read(cert_path)?;

    let mut crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(SkipServerVerification::new())
        .with_no_client_auth();
    // Configure SSLKEYLOGFILE for rustls and debugin via Wireshark
    crypto.key_log = Arc::new(rustls::KeyLogFile::new());

    let mut client_cfg = ClientConfig::new(Arc::new(crypto));
    let mut transport_cfg = quinn::TransportConfig::default();
    transport_cfg
        .max_concurrent_uni_streams(VarInt::from_u32(32))
        .max_concurrent_bidi_streams(VarInt::from_u32(32))
        .datagram_receive_buffer_size(Some(65535))
        .congestion_controller_factory(Arc::new(
            quinn::congestion::BbrConfig::default(),
        ))
        .send_window(32500)
        .max_idle_timeout(None);
    client_cfg.transport_config(Arc::new(transport_cfg));

    Ok(client_cfg)
}
#[allow(clippy::field_reassign_with_default)] // https://github.com/rust-lang/rust-clippy/issues/6527
fn configure_server() -> Result<(ServerConfig, Vec<u8>)> {
    let (cert, key) = match std::fs::read("./cert.der")
        .and_then(|x| Ok((x, std::fs::read("./key.der")?)))
    {
        Ok(x) => x,
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
            let cert =
                rcgen::generate_simple_self_signed(vec!["localhost".into()])
                    .unwrap();
            let key = cert.serialize_private_key_der();
            let cert = cert.serialize_der().unwrap();
            std::fs::write("./cert.der", &cert)
                .expect("Failed to write cert to ./cert.der");
            std::fs::write("./key.der", &key)
                .expect("Failed to write key to ./key.der");
            (cert, key)
        }
        Err(_) => panic!("Error reading or generating certificates"),
    };
    let key = rustls::PrivateKey(key);
    let cert_chain = vec![rustls::Certificate(cert.clone())];

    let mut server_config = ServerConfig::with_single_cert(cert_chain, key)?;
    Arc::get_mut(&mut server_config.transport)
        .unwrap()
        .max_concurrent_uni_streams(VarInt::from_u32(32))
        .max_concurrent_bidi_streams(VarInt::from_u32(32))
        .congestion_controller_factory(Arc::new(
            quinn::congestion::BbrConfig::default(),
        ))
        .datagram_receive_buffer_size(Some(65535))
        .max_idle_timeout(None);

    Ok((server_config, cert))
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn test_connection() -> Result<()> {
        let mut futures = Vec::new();
        let server = QuicServer::new(("127.0.0.1".parse()?, 0))?;
        let server_addr = server.server.local_addr()?.clone();

        futures.push(tokio::spawn(async move {
            let incomming = server.server.accept();

            let connection = incomming
                .await
                .ok_or_else(|| anyhow::anyhow!("Failed to accept"))?;

            let connection = connection.await?;
            dbg!("Accepted connection");
            let bi_stream = connection.accept_bi().await?;
            let (mut _send, mut recv) = bi_stream;
            let mut buf = [0u8; 10];
            recv.read(&mut buf).await?;
            assert_eq!(buf, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
            let dgram = connection.read_datagram().await?;
            assert_eq!(dgram, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
            dbg!("Received data");
            Ok::<(), anyhow::Error>(())
        }));

        dbg!("Server started");

        let client = QuicClient::new(None, None, None)?;

        let con = client
            .connect((server_addr.ip(), server_addr.port()))
            .await?;
        con.send_datagram(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10].into())?;
        let (mut send, mut _recv) = con.open_bi().await?;
        let snd_len = send.write(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]).await?;
        assert_eq!(snd_len, 10);

        for fut in futures {
            fut.await??;
        }

        Ok(())
    }
}
