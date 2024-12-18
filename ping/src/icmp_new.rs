use std::{
    net::IpAddr,
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

use anyhow::{anyhow, Result};
use common::{
    interface_to_ipaddr, AsyncICMPSocket, ICMPSocket, Logger, Statistics,
};
use etherparse::{IcmpEchoHeader, Icmpv4Header, Icmpv4Type};
use tokio_util::bytes::{Bytes, BytesMut};

use crate::{args, logger::PingResult};
pub struct Test {
    /// Logger
    logger: Option<Logger<PingResult>>,
    /// Common options
    common: args::CommonOpts,
    /// ICMP socket
    socket: common::ICMPSocket,
    /// Src IP address of the socket
    src_addr: IpAddr,
    /// Destination IP address
    dst_addr: IpAddr,
    /// Internal counter for sequence number of ICMP packets (Not the same as
    /// ip sequence number)
    internal_couter: u128,
    /// Identifier of ICMP packets (This is random by default)
    identifier: u16,
    /// Rtt statistics
    rtt_stats: Statistics,
}

struct HandleClient<Echo, Reply> {
    tx: tokio::sync::mpsc::Sender<Echo>,
    rx: tokio::sync::mpsc::Receiver<Reply>,
    stop: tokio::sync::oneshot::Sender<()>,
}
impl HandleClient<ICMPEcho, PingResult> {
    fn new(args: args::ICMPOpts) -> Self {
        let echo_queue = tokio::sync::mpsc::channel::<ICMPEcho>(100);
        let reply_queue = tokio::sync::mpsc::channel::<PingResult>(100);
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();

        let client =
            PingClient::new(args, echo_queue.1, reply_queue.0, stop_rx);
        run_icmp_pinger(client.unwrap()).unwrap();

        Self {
            tx: echo_queue.0,
            rx: reply_queue.1,
            stop: stop_tx,
        }
    }
}

struct PingClient<T, Echo, Reply> {
    socket: T,
    send_queue: tokio::sync::mpsc::Receiver<Echo>,
    recv_queue: tokio::sync::mpsc::Sender<Reply>,
    stop: tokio::sync::oneshot::Receiver<()>,
    identifier: u16,
}

struct ICMPEcho {
    icmp: IcmpEchoHeader,
    payload: Bytes,
}

type ICMPPinger = PingClient<AsyncICMPSocket, ICMPEcho, PingResult>;

impl PingClient<AsyncICMPSocket, ICMPEcho, PingResult> {
    pub fn new(
        args: args::ICMPOpts,
        send_queue: tokio::sync::mpsc::Receiver<ICMPEcho>,
        recv_queue: tokio::sync::mpsc::Sender<PingResult>,
        stop: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<Self> {
        let iface = args.common_opts.iface.clone();

        let src_addr = match args.common_opts.src_addr {
            Some(addr) => addr,
            None => interface_to_ipaddr(iface.as_ref().unwrap())?,
        };

        let dst_addr = args.common_opts.dst_addr;
        let socket = ICMPSocket::new(iface.as_deref(), Some(src_addr))?;
        socket.connect(dst_addr)?;

        Ok(PingClient {
            socket: AsyncICMPSocket::new(socket)?,
            send_queue,
            recv_queue,
            stop,
            identifier: rand::random::<u16>(),
        })
    }
}

fn run_icmp_pinger(mut pinger: ICMPPinger) -> Result<()> {
    tokio::spawn(async move {
        let mut buffer = BytesMut::with_capacity(65_535);
        loop {
            tokio::select! {
                            Some(echo) = pinger.send_queue.recv() => {
                                let icmp_packet = {
                                    [
                                        Icmpv4Header::with_checksum(
                                            Icmpv4Type::EchoRequest(
                                            echo.icmp),
                                            echo.payload.as_ref()// Payload for checksum
                                        )
                                        .to_bytes()
                                        .as_slice(),
                                        echo.payload.as_ref(),
                                    ]
                                    .concat() // Concatenate header and payload
                                };

                                pinger.socket.send(&icmp_packet).await?;
                            },
                            Ok(len) = pinger.socket.read(&mut buffer) => {
                                if let Ok(ping_result) = parse_icmp_packet(&buffer, len) {
                                    pinger.recv_queue.send(ping_result).await?;
                                }
            }
                            _ =&mut pinger.stop => { break; },
                        }
        }
        Ok::<(), anyhow::Error>(())
    });
    Ok(())
}

fn parse_icmp_packet(buffer: &[u8], len: usize) -> Result<PingResult> {
    let icmp_header = Icmpv4Header::from_slice(&buffer[..len])?;
    let reply_header = match icmp_header.0.icmp_type {
        Icmpv4Type::EchoReply(header) => {
            // Check if the packet is a reply to our packet
            if header.id == identifier {
                header
            } else {
                println!("Received reply, but not our packet");
                return None;
            }
        }
        _ => {
            println!("Received non-echo reply packet");
            return None;
        }
    };
    let ping_result = PingResult {
        seq: icmp_echo.sequence_number,
        unique_seq: icmp_echo.identifier as u128,
        ttl: icmp_packet.ttl,
        rtt: icmp_echo.timestamp as f64,
        size: len,
        send_timestamp: icmp_echo.timestamp,
        recv_timestamp: icmp_echo.timestamp_reply,
        dst_addr: pinger.socket.dst_addr().to_string(),
        src_addr: pinger.socket.src_addr().to_string(),
    };
}
