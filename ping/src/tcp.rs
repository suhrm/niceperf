use std::{
    collections::HashMap,
    net::IpAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, UDPSocket};
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket as tokioUdpSocket;

use crate::{
    args,
    logger::{PingLogger, UDPEchoResult},
};

pub struct TCPClient {
    socket: tokio::net::UdpSocket,
    common: args::CommonOpts,
    src_addr: IpAddr,
    dst_addr: IpAddr,
    internal_couter: u128,
    identifier: u16,
    src_port: Option<u16>,
    dst_port: u16,
    logger: Option<PingLogger>,
}

impl TCPClient {
    pub fn new(args: args::UDPOpts) -> Result<TCPClient> {
        let iface = args
            .common_opts
            .iface
            .clone()
            .ok_or(anyhow!("No interface specified"))?;
        let iface = iface.as_str();
        let src_addr = interface_to_ipaddr(iface)?;
        let dst_addr = args.dst_addr;
        let dst_port = args.dst_port;

        let socket = UDPSocket::new(Some(iface), None)?;
        let socket = tokioUdpSocket::from_std(
            socket.get_ref().try_clone()?.try_into()?,
        )?;

        let logger = match args.common_opts.file.clone() {
            Some(file_name) => Some(PingLogger::new(file_name)?),
            None => None,
        };

        Ok(TCPClient {
            socket,
            common: args.common_opts,
            src_addr,
            dst_addr,
            internal_couter: 0,
            identifier: rand::random::<u16>(),
            src_port: None,
            dst_port,
            logger,
        })
    }
    pub async fn run(&mut self) -> Result<()> {
        let dst_addr = match self.dst_addr {
            IpAddr::V4(addr) => addr,
            IpAddr::V6(_) => {
                return Err(anyhow!("IPv6 is not supported yet"));
            }
        };
        let src_addr = match self.src_addr {
            IpAddr::V4(addr) => addr,
            IpAddr::V6(addr) => {
                return Err(anyhow!("IPv6 is not supported yet"));
            }
        };

        let mut buf = [0u8; 1500];

        // Safety: Safe to unwrap because we have a default value
        println!(
            "Pinging {} with {} bytes of data",
            dst_addr,
            self.common.len.unwrap()
        );
        println!(
            "interval {:?} preload {:?}",
            self.common.interval, self.common.preload
        );
        // TODO: Add support for timeout
        let timeout_tracker =
            tokio::time::interval(std::time::Duration::from_millis(10 * 1000));
        let mut pacing_timer = tokio::time::interval(
            std::time::Duration::from_millis(self.common.interval.unwrap()),
        );
        // TODO: Generate a random payload for the ICMP packet

        // Safety: Safe to unwrap because we have a default value
        let mut payload = TcpEchoPacket {
            identifier: self.identifier,
            seq: 0,
            send_timestamp: 0,
            recv_timestamp: 0,
            payload: vec![0u8; self.common.len.unwrap() as usize],
        };

        loop {
            tokio::select! {
                _ = pacing_timer.tick() => {
                    // Build UDP echo packet
                    payload.seq = self.internal_couter;
                    payload.send_timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
                    let echo_packet = bincode::serialize(&payload)?;
                    self.socket.send_to(&echo_packet, (self.dst_addr, self.dst_port)).await?;
                    self.internal_couter += 1;
                },
                Ok((len, recv_addr)) = self.socket.recv_from(&mut buf) => {
                    // Deserialize the packet
                    let recv_packet: TcpEchoPacket = bincode::deserialize(&buf[..len])?;

                    // Puplate the result

                    let recv_timestamp= SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
                    let rtt = ((recv_timestamp - recv_packet.send_timestamp) as f64)/1e6;
                    let result = UDPEchoResult {
                        recv_timestamp,

                        seq: recv_packet.seq,
                        send_timestamp: recv_packet.send_timestamp,
                        server_timestamp: recv_packet.recv_timestamp,
                        rtt: rtt,
                        src_addr: self.src_addr.to_string(),
                        dst_addr: self.dst_addr.to_string(),
                    };

               if self.logger.is_some() {
                   // Safety: Safe to unwrap because the file is some
                   self.logger.as_mut().unwrap().log(&result).await?;
               }



                }

            }
        }
    }
}
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct TcpEchoPacket {
    identifier: u16,
    send_timestamp: u128,
    recv_timestamp: u128,
    seq: u128,
    #[serde(with = "serde_bytes")]
    payload: Vec<u8>,
}

pub struct TCPServer {
    socket: tokio::net::UdpSocket,
    common: args::CommonOpts,
    dst_addr: IpAddr,
    internal_couter: u128,
    dst_port: u16,
    clients: HashMap<u16, (IpAddr, u16)>,
}

impl TCPServer {
    pub fn new(args: args::TCPOpts) -> Result<TCPServer> {
        let dst_addr = args.dst_addr;
        let dst_port = args.dst_port;

        let socket = UDPSocket::new(None, Some((dst_addr, dst_port)))?;
        let socket = tokioUdpSocket::from_std(
            socket.get_ref().try_clone()?.try_into()?,
        )?;

        Ok(TCPServer {
            socket,
            common: args.common_opts,
            dst_addr,
            internal_couter: 0,
            dst_port,
            clients: HashMap::new(),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut buf = [0u8; 1500];

        loop {
            let (len, recv_addr) = self.socket.recv_from(&mut buf).await?;
            let mut recv_packet: TcpEchoPacket =
                bincode::deserialize(&buf[..len])?;
            let recv_timestamp =
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
            recv_packet.recv_timestamp = recv_timestamp;
            let echo_packet = bincode::serialize(&recv_packet)?;
            self.socket.send_to(&echo_packet, recv_addr).await?;
        }
    }
}
