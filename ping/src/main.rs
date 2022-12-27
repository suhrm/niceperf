use clap::Parser;
mod args;
use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, ICMPSocket};
use core::mem::MaybeUninit;
use etherparse::{
    ip_number, Icmpv4Type, IpHeader, Ipv4Header, Ipv4HeaderSlice,
    PacketBuilder, TransportHeader,
};
use socket2::Socket;
use std::net::IpAddr;
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

        loop {
            tokio::select! {
                _ = pacing_timer.tick() => {
                    println!("Tick");
                    println!("Sending packet");
                    println!("Internal counter: {}", self.internal_couter);
                    println!("dst_addr: {}", dst_addr);
                    println!("src_addr: {}", src_addr);
                    // Build ICMP packet
                    // TODO: We have an ICMP socket so we dont need to build ip
                    // but PacketBuilder expects it as an entry point.
                    let builder = PacketBuilder::ipv4(
                                src_addr.octets(),
                                dst_addr.octets(),
                                64,
                        ).icmpv4_raw(
                            8,
                            0,
                            [1, 2, 3, 4],
                        );
                    let payload = b"Hello world";
                    let mut packet = Vec::with_capacity(builder.size(payload.len()));
                    builder.write(&mut packet,payload).unwrap();
                    println!("Packet: {:x?}", packet);
                    self.socket.send_to(&packet[20..], &self.dst_addr).await?;
                    self.internal_couter += 1;






                },
                _ = self.socket.read(&mut buf) => { }
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
        println!("Addr: {:?}", addr);
        let addr = match addr {
            IpAddr::V4(addr) => {
                let addr = std::net::SocketAddr::V4(
                    std::net::SocketAddrV4::new(*addr, 0),
                );
                println!("Addr: {:?}", addr);
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
        println!("Sending packet");
        println!("Packet: {:x?}", packet);
        println!("Addr: {:?}", addr);
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
