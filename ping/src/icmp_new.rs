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
use serde::{Deserialize, Serialize};
use tokio_util::bytes::{Bytes, BytesMut};

use crate::{args, logger::PingResult};
pub struct PingerClient {
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

pub struct HandleClient<Echo, Reply> {
    tx: tokio::sync::mpsc::Sender<Echo>,
    rx: tokio::sync::mpsc::Receiver<Reply>,
    identifier: u16,
    stop: tokio::sync::oneshot::Sender<()>,
}

pub type ICMPClientHandle = HandleClient<ICMPEcho, PingResult>;

impl HandleClient<ICMPEcho, PingResult> {
    pub fn new(args: &args::ICMPOpts) -> Self {
        let echo_queue = tokio::sync::mpsc::channel::<ICMPEcho>(100);
        let reply_queue = tokio::sync::mpsc::channel::<PingResult>(100);
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();

        let identifier = rand::random::<u16>();

        let client = PingClient::new(
            args,
            echo_queue.1,
            reply_queue.0,
            stop_rx,
            identifier,
        )
        .unwrap();
        run_icmp_pinger(client).unwrap();

        Self {
            tx: echo_queue.0,
            rx: reply_queue.1,
            stop: stop_tx,
            identifier,
        }
    }
    pub fn stop(&self) {
        unimplemented!("Stop not implemented")
    }
    pub async fn send(&self, echo: ICMPEcho) -> Result<()> {
        self.tx
            .send(echo)
            .await
            .map_err(|_| anyhow!("Failed to send"))?;
        Ok(())
    }
    pub async fn recv(&mut self) -> Option<PingResult> {
        self.rx.recv().await
    }
    pub fn identifier(&self) -> u16 {
        self.identifier
    }
}

struct PingClient<Transport, Echo, Reply> {
    socket: Transport,
    send_queue: tokio::sync::mpsc::Receiver<Echo>,
    recv_queue: tokio::sync::mpsc::Sender<Reply>,
    stop: tokio::sync::oneshot::Receiver<()>,
    identifier: u16,
}

pub struct ICMPEcho {
    pub icmp: IcmpEchoHeader,
    pub payload: Payload, /* make the payload a struct instead of "just"
                           * abitrairy bytes as we expect a specific format
                           * anyway */
}
#[derive(Serialize, Deserialize)]
pub struct Payload {
    pub data: Vec<u8>,
    pub seq: u128,
    pub timestamp: u128,
}

type ICMPPinger = PingClient<AsyncICMPSocket, ICMPEcho, PingResult>;

impl PingClient<AsyncICMPSocket, ICMPEcho, PingResult> {
    pub fn new(
        args: &args::ICMPOpts,
        send_queue: tokio::sync::mpsc::Receiver<ICMPEcho>,
        recv_queue: tokio::sync::mpsc::Sender<PingResult>,
        stop: tokio::sync::oneshot::Receiver<()>,
        identifier: u16,
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
            identifier,
        })
    }
}

fn run_icmp_pinger(mut pinger: ICMPPinger) -> Result<()> {
    tokio::spawn(async move {
        let mut buffer = [0u8; 65536];
        loop {
            tokio::select! {
                            Some(echo) = pinger.send_queue.recv() => {
                                let payload_bytes = bincode::serialize(&echo.payload)?;
                                let icmp_packet = {
                                    [
                                        Icmpv4Header::with_checksum(
                                            Icmpv4Type::EchoRequest(
                                            echo.icmp),
                                            payload_bytes.as_slice()// Payload for checksum
                                        )
                                        .to_bytes()
                                        .as_slice(),
                                            payload_bytes.as_slice()// Payload for checksum
                                    ]
                                    .concat() // Concatenate header and payload
                                };

                                pinger.socket.send(&icmp_packet).await?;
                            },
                            Ok(len) = pinger.socket.read(&mut buffer) => {
                                if let Ok(ping_result) = parse_icmp_packet(&buffer, len, pinger.identifier) {
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

fn parse_icmp_packet(
    buffer: &[u8],
    len: usize,
    identifier: u16,
) -> Result<PingResult> {
    let ipv4_hdr = etherparse::Ipv4HeaderSlice::from_slice(&buffer[..20])?;
    let icmp_header = Icmpv4Header::from_slice(&buffer[20..len])?;
    let reply_header = match icmp_header.0.icmp_type {
        Icmpv4Type::EchoReply(header) => {
            // Check if the packet is a reply to our packet
            if header.id == identifier {
                header
            } else {
                return Err(anyhow!("Received reply, but not our packet"));
            }
        }
        _ => {
            return Err(anyhow!("Received non-echo reply"));
        }
    };
    let reply_payload = bincode::deserialize::<Payload>(icmp_header.1)?;
    let recv_timestamp = std::time::Duration::from(nix::time::clock_gettime(
        nix::time::ClockId::CLOCK_MONOTONIC,
    )?)
    .as_nanos() as u128;
    let send_timestamp = reply_payload.timestamp;
    let rtt = ((recv_timestamp - send_timestamp) as f64) / 1e6;
    let ttl = buffer[8];
    let seq_internal = reply_payload.seq;
    let ping_result = PingResult {
        seq: reply_header.seq,
        unique_seq: seq_internal,
        ttl,
        rtt,
        size: len,
        send_timestamp,
        recv_timestamp,
        dst_addr: ipv4_hdr.destination_addr().to_string(),
        src_addr: ipv4_hdr.source_addr().to_string(),
    };
    Ok(ping_result)
}
