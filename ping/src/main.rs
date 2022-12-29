use clap::Parser;
mod args;
use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, ICMPSocket};
use core::mem::MaybeUninit;
use etherparse::{
    ip_number, IcmpEchoHeader, Icmpv4Header, Icmpv4Type, IpHeader,
    Ipv4HeaderSlice, PacketBuilder, TransportHeader,
};
use socket2::Socket;
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::unix::AsyncFd;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = args::Opts::parse();

    match args.mode {
        args::Modes::Client { proto } => match proto {
            args::Protocol::Tcp(opts) => {
                println!("TCP");
            }
            args::Protocol::Udp(opts) => {
                println!("UDP");
            }
            args::Protocol::Icmp(opts) => {
                let client = ICMPClient::new(opts);
                client?.run().await?;
            }
        },
        args::Modes::Server { proto } => match proto {
            args::Protocol::Tcp(opts) => {
                println!("TCP");
            }
            args::Protocol::Udp(opts) => {
                println!("UDP");
            }
            args::Protocol::Icmp(opts) => {
                println!("ICMP");
            }
        },
    };
    Ok(())
    // match args.mode {}
}

struct ICMPClient {
    /// ICMP socket
    socket: AsyncICMPSocket,
    /// Interface to bind to
    iface: String,
    /// Src IP address of the socket
    src_addr: IpAddr,
    /// Destination IP address
    dst_addr: IpAddr,
    /// Internal counter for sequence number of ICMP packets (Not the same as ip sequence number)
    internal_couter: u128,
    /// How many packets preload before following specified interval.
    preload: u64,
    /// Payload size of each packet
    payload_size: usize,
    /// Identifier of ICMP packets (This is random by default)
    identifier: u16,
}

impl ICMPClient {
    pub fn new(args: args::ICMPOpts) -> Result<ICMPClient> {
        let iface = args
            .common_opts
            .iface
            .ok_or(anyhow!("No interface specified"))?;
        let iface = iface.as_str();
        let src_addr = interface_to_ipaddr(iface)?;
        let dst_addr = args.dst_addr;
        let socket = ICMPSocket::new(Some(iface), None)?;

        Ok(ICMPClient {
            socket: AsyncICMPSocket::new(socket)?,
            iface: iface.to_string(),
            src_addr,
            dst_addr,
            internal_couter: 0,
            // TODO: Check what ping does
            preload: args.preload.unwrap_or(1),
            payload_size: args.common_opts.len.unwrap_or(56),
            identifier: rand::random::<u16>(),
        })
    }
    pub async fn run(&mut self) -> Result<()> {
        let mut pacing_timer =
            tokio::time::interval(std::time::Duration::from_millis(1000));
        let dst_addr = match self.dst_addr {
            IpAddr::V4(addr) => addr,
            IpAddr::V6(_) => {
                return Err(anyhow!("IPv6 is not supported yet"));
            }
        };
        let src_addr = match self.src_addr {
            IpAddr::V4(addr) => addr,
            IpAddr::V6(_) => {
                return Err(anyhow!("IPv6 is not supported yet"));
            }
        };

        let mut buf = [0u8; 1500];

        let timeout_tracker =
            tokio::time::interval(std::time::Duration::from_millis(1000));
        let mut payload = vec![0u8; self.payload_size - 8];

        loop {
            tokio::select! {
                _ = pacing_timer.tick() => {
                    // Build ICMP packet
                    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
                    // Put timestamp in the payload
                    payload[..16].copy_from_slice(&timestamp.to_be_bytes());

                    let icmp_packet = [Icmpv4Header::with_checksum(
                        Icmpv4Type::EchoRequest(IcmpEchoHeader {
                            id: self.identifier, // Identifier
                            seq: self.internal_couter as u16, // Sequence number
                        }),
                        payload.as_slice(), // Payload for checksum
                    ).to_bytes().as_slice(), payload.as_slice()].concat(); // Concatenate header and payload
                    self.socket.send_to(&icmp_packet, &self.dst_addr).await?;
                    self.internal_couter += 1;
                },
                Ok(len) = self.socket.read(&mut buf) => {
                // TODO: Handle ICMP packet
               let icmp_header = Icmpv4Header::from_slice(&buf[20..len])?;

               let reply_header = match icmp_header.0.icmp_type {
                   Icmpv4Type::EchoReply(header) => {
                       // Check if the packet is a reply to our packet
                       if header.id == self.identifier {
                           header
                          } else {
                              println!("Received reply, but not our packet");
                                continue;

                            }
                   },
                   _ => {
                       println!("Received non-echo reply packet");
                       continue;
                   }
               };

               let reply_payload = icmp_header.1;
               let reply_timestamp = u128::from_be_bytes(reply_payload[..16].try_into()?);
               let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
               let rtt = ((timestamp - reply_timestamp) as f64)/ 1e6;

               println!("{} bytes from {}: icmp_seq={} ttl={} time={} ms", len -20 , dst_addr, reply_header.seq, buf[8], rtt );



                }

            }
        }
    }
}

struct AsyncICMPSocket {
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
