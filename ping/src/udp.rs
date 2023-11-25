use std::{
    collections::HashMap,
    net::IpAddr,
    ops::{Add, AddAssign},
    os::fd::{AsRawFd, FromRawFd},
    sync::{atomic::AtomicUsize, Arc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, Logger, Statistics, UDPSocket};
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket as tokioUdpSocket;

use crate::{args, logger::UDPEchoResult};

pub struct UDPClient {
    socket: common::UDPSocket,
    common: args::CommonOpts,
    src_addr: IpAddr,
    dst_addr: IpAddr,
    internal_couter: u128,
    identifier: u16,
    src_port: Option<u16>,
    dst_port: u16,
    logger: Option<Logger<UDPEchoResult>>,
    rtt_stats: Statistics,
}

impl UDPClient {
    pub fn new(args: args::UDPOpts) -> Result<UDPClient> {
        let iface = match args.common_opts.iface.clone() {
            Some(iface) => Some(iface),
            None => None,
        };
        let src_addr = match args.common_opts.src_addr {
            Some(addr) => addr,
            None => interface_to_ipaddr(iface.as_ref().unwrap())?,
        };

        let src_port = match args.src_port {
            Some(port) => port,
            None => 0,
        };

        let dst_addr = args.common_opts.dst_addr;
        let dst_port = args.dst_port;

        let socket =
            UDPSocket::new(iface.as_deref(), Some((src_addr, src_port)))?;
        socket.connect((dst_addr, dst_port))?;

        let logger = match args.common_opts.file.clone() {
            Some(file_name) => Some(Logger::new(file_name)?),
            None => None,
        };

        Ok(UDPClient {
            socket,
            common: args.common_opts,
            src_addr,
            dst_addr,
            internal_couter: 0,
            identifier: rand::random::<u16>(),
            src_port: None,
            dst_port,
            logger,
            rtt_stats: Statistics::new(),
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
            IpAddr::V6(_addr) => {
                return Err(anyhow!("IPv6 is not supported yet"));
            }
        };

        let _buf = [0u8; u16::MAX as usize];

        // Safety: Safe to unwrap because we have a default value
        println!(
            "Pinging {} with {} bytes of data",
            dst_addr,
            self.common.len.unwrap()
        );
        println!(
            "interval {} ms, preload {} packets ",
            self.common.interval.unwrap(),
            self.common.preload.unwrap()
        );
        // TODO: Add support for timeout
        let _timeout_tracker =
            tokio::time::interval(std::time::Duration::from_millis(10 * 1000));
        let _pacing_timer = tokio::time::interval(
            std::time::Duration::from_millis(self.common.interval.unwrap()),
        );
        // TODO: Generate a random payload for the ICMP packet

        // Safety: Safe to unwrap because we have a default value
        let mut payload = UdpEchoPacket {
            identifier: self.identifier,
            seq: 0,
            send_timestamp: 0,
            recv_timestamp: 0,
            payload: vec![0u8; self.common.len.unwrap() as usize],
        };
        // Recv counter
        let mut send_seq = 0;
        let mut recv_counter = 0;
        let interval = self.common.interval.unwrap();
        let num_ping = self.common.count.take();

        let tx = tokio::net::UdpSocket::from_std(
            self.socket.get_ref().try_clone()?.try_into()?,
        )?;

        let rx = tokio::net::UdpSocket::from_std(
            self.socket.get_ref().try_clone()?.try_into()?,
        )?;

        let send_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let mut pacing_timer = tokio::time::interval(
                std::time::Duration::from_millis(interval),
            );
            loop {
                if let Some(num_ping) = num_ping {
                    if send_seq > num_ping {
                        println!("Sent {} packets", num_ping);
                        break;
                    }
                }
                _ = pacing_timer.tick().await;
                payload.seq = send_seq as u128;
                payload.send_timestamp =
                    std::time::Duration::from(nix::time::clock_gettime(
                        nix::time::ClockId::CLOCK_MONOTONIC,
                    )?)
                    .as_nanos() as u128;
                let encoded_packet = bincode::serialize(&payload)?;
                tx.send(encoded_packet.as_slice()).await?;
                send_seq = send_seq + 1;
            }
            Ok::<(), anyhow::Error>(())
        });

        let mut logger = self.logger.take();
        let log = self.common.log.take().unwrap();
        let recv_task = tokio::spawn(async move {
            let mut stats = Statistics::new();
            let mut result = UDPEchoResult {
                seq: 0,
                rtt: 0.0,
                send_timestamp: 0,
                recv_timestamp: 0,
                server_timestamp: 0,
                src_addr: src_addr.to_string(),
                dst_addr: dst_addr.to_string(),
                size: 0,
            };

            let (log_writer, mut log_reader) =
                tokio::sync::mpsc::channel::<UDPEchoResult>(10000);
            let mut time = 0;
            let mut stat_piat = Statistics::new();
            tokio::spawn(async move {
                while let Some(result) = log_reader.recv().await {
                    if recv_counter == 0 {
                        time = result.recv_timestamp;
                    } else {
                        let diff = (result.recv_timestamp - time) / 1000000;
                        stat_piat.update(diff as f64);
                        time = result.recv_timestamp;
                    }
                    stats.update(result.rtt);
                    if logger.is_some() {
                        logger.as_mut().unwrap().log(&result).await?;
                        if recv_counter % 100 == 0 {
                            println!("RTT: {}", stats);
                            println!("PIAT: {}", stat_piat);
                        }
                    } else if log {
                        println!(
                            "{} bytes from {}: udp_pay_seq={} time={:.3} ms ",
                            result.size,
                            result.src_addr,
                            result.seq,
                            result.rtt,
                        );
                    } else {
                        if recv_counter % 100 == 0 {
                            println!("RTT: {}", stats);
                            println!("PIAT: {}", stat_piat);
                        }
                    }
                    recv_counter += 1;
                }
                Ok::<(), anyhow::Error>(())
            });
            let mut buf = [0u8; u16::MAX as usize];
            while let Ok(len) = rx.recv(&mut buf).await {
                let recv_timestamp =
                    std::time::Duration::from(nix::time::clock_gettime(
                        nix::time::ClockId::CLOCK_MONOTONIC,
                    )?)
                    .as_nanos() as u128;
                let decoded_packet: UdpEchoPacket =
                    bincode::deserialize(&buf[..len])?;
                // Get the current socket stats

                let send_timestamp = decoded_packet.send_timestamp;
                let seq = decoded_packet.seq;
                let rtt = ((recv_timestamp - send_timestamp) as f64) / 1e6;

                result.seq = seq;
                result.rtt = rtt;
                result.send_timestamp = send_timestamp;
                result.recv_timestamp = recv_timestamp;
                result.server_timestamp = decoded_packet.recv_timestamp;
                result.size = len as usize;

                log_writer.send(result.clone()).await?;
            }
            Ok::<(), anyhow::Error>(())
        });

        tokio::select! {
            _ = send_task => {
                println!("Send stop");
                if recv_counter < send_seq {
                    println!("Sent {} packets, but only received {} packets", send_seq, recv_counter);
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            },
            failure = recv_task => {
                panic!("Recv task failed: {:?}", failure);

            }
        }

        Ok(())
    }
}
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct UdpEchoPacket {
    identifier: u16,
    send_timestamp: u128,
    recv_timestamp: u128,
    seq: u128,
    #[serde(with = "serde_bytes")]
    payload: Vec<u8>,
}

pub struct UDPServer {
    socket: tokio::net::UdpSocket,
    common: args::CommonOpts,
    dst_addr: IpAddr,
    internal_couter: u128,
    dst_port: u16,
    clients: HashMap<u16, (IpAddr, u16)>,
}

impl UDPServer {
    pub fn new(args: args::UDPOpts) -> Result<UDPServer> {
        let dst_addr = args.common_opts.dst_addr;
        let dst_port = args.dst_port;

        let mut socket = UDPSocket::new(None, Some((dst_addr, dst_port)))?;
        socket.get_mut().set_reuse_address(true)?;
        let socket = tokioUdpSocket::from_std(
            socket.get_ref().try_clone()?.try_into()?,
        )?;

        Ok(UDPServer {
            socket,
            common: args.common_opts,
            dst_addr,
            internal_couter: 0,
            dst_port,
            clients: HashMap::new(),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut buf = [0u8; u16::MAX as usize];
        loop {
            let (len, recv_addr) = self.socket.recv_from(&mut buf).await?;

            let mut recv_packet: UdpEchoPacket =
                bincode::deserialize(&buf[..len])?;
            let recv_timestamp =
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            recv_packet.recv_timestamp = recv_timestamp;
            let echo_packet = bincode::serialize(&recv_packet)?;
            self.socket.send_to(&echo_packet, recv_addr).await?;

            let mut client_sock = common::UDPSocket::new(None, None)?;
            client_sock.get_mut().set_reuse_address(true)?;
            client_sock.bind(self.socket.local_addr()?)?;
            let client_sock = tokioUdpSocket::from_std(
                client_sock.get_ref().try_clone()?.try_into()?,
            )?;

            client_sock.connect(recv_addr).await?;
            tokio::spawn(async move {
                println!("New client: {:?}", recv_addr);
                match Self::handle_client(client_sock).await {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Error: {:?}:{:?}", e, recv_addr);
                    }
                }
            });
        }
    }

    pub async fn handle_client(
        client_sock: tokio::net::UdpSocket,
    ) -> Result<()> {
        let mut buf = [0u8; u16::MAX as usize];
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(10)) => {
                    Err(anyhow!("Client timeout"))?;
                },
                Ok(len) = client_sock.recv(&mut buf) => {
                    let mut recv_packet: UdpEchoPacket =
                        bincode::deserialize(&buf[..len])?;
                    let recv_timestamp =
                        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
                    recv_packet.recv_timestamp = recv_timestamp;
                    let echo_packet = bincode::serialize(&recv_packet)?;
                    client_sock.send(&echo_packet).await?;
                }
                else => {
                    Err(anyhow!("Client error"))?;
                }
            }
        }
    }
}
