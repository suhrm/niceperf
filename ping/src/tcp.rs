use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, Logger, Statistics, TCPSocket};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::{
    TcpListener as TokioTcpListener, TcpStream as TokioTcpStream,
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::{args, logger::TCPEchoResult};

pub struct TCPClient {
    common: args::CommonOpts,
    socket: common::TCPSocket,
    src_addr: IpAddr,
    dst_addr: IpAddr,
    identifier: u16,
    src_port: Option<u16>,
    dst_port: u16,
    logger: Option<Logger<TCPEchoResult>>,
    cc: String,
}

impl TCPClient {
    pub fn new(args: args::TCPOpts) -> Result<TCPClient> {
        let iface = args
            .common_opts
            .iface
            .clone()
            .ok_or(anyhow!("No interface specified"))?;
        let iface = iface.as_str();
        let src_addr = interface_to_ipaddr(iface)?;
        let dst_addr = args.dst_addr;
        let dst_port = args.dst_port;

        let mut socket =
            TCPSocket::new(Some(iface), None, args.mss, args.cc.clone())?;
        println!("Trying to connect to {}...", dst_addr);
        socket.connect(SocketAddr::new(dst_addr, dst_port))?;
        println!("Connected to {}", dst_addr);
        let logger = match args.common_opts.file.clone() {
            Some(file_name) => Some(Logger::new(file_name)?),
            None => None,
        };

        Ok(TCPClient {
            socket,
            src_addr,
            dst_addr,
            identifier: rand::random::<u16>(),
            src_port: None,
            dst_port,
            logger,
            common: args.common_opts,
            // Safety we have a default value
            cc: args.cc.unwrap(),
        })
    }
    pub async fn run(
        &mut self,
        mut data_ch: Option<std::sync::mpsc::Sender<f64>>,
    ) -> Result<()> {
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
        // TODO: Generate a random payload for the ICMP packet
        let mut payload = TcpEchoPacket {
            identifier: self.identifier,
            seq: 0,
            send_timestamp: 0,
            recv_timestamp: 0,
            payload: vec![0u8; self.common.len.unwrap() as usize],
        };

        // Convert to tokio TcpStream
        let socket = TokioTcpStream::from_std(
            self.socket.get_ref().try_clone()?.try_into()?,
        )?;

        // Convert to framed readers and writers as TCP is a streaming protocol
        let (rx, tx) = socket.into_split();
        let mut rx = FramedRead::new(rx, LengthDelimitedCodec::new());
        let mut tx = FramedWrite::new(tx, LengthDelimitedCodec::new());

        // Recv counter
        let mut recv_counter = 0;
        let mut send_seq = 0;
        let (send_stop, mut recv_stop) = tokio::sync::mpsc::channel::<()>(1);
        let interval = self.common.interval.unwrap();
        let num_ping = self.common.count.take();

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
                tx.send(encoded_packet.into()).await?;
                send_seq = send_seq + 1;
            }
            Ok::<(), anyhow::Error>(())
        });

        let cc = self.cc.clone();
        let mut logger = self.logger.take();
        let log = self.common.log.take().unwrap();
        let raw_socket =
            TCPSocket::from_raw_socket(self.socket.get_ref().try_clone()?)?;
        let recv_task = tokio::spawn(async move {
            let mut stats = Statistics::new();
            let mut result = TCPEchoResult {
                seq: 0,
                rtt: 0.0,
                send_timestamp: 0,
                recv_timestamp: 0,
                server_timestamp: 0,
                src_addr: src_addr.to_string(),
                dst_addr: dst_addr.to_string(),
                cc: cc.clone(),
                size: 0,
                retrans: 0,
            };

            let (log_writer, mut log_reader) =
                tokio::sync::mpsc::channel::<TCPEchoResult>(10000);
            let mut time = 0;
            let mut stat_piat = Statistics::new();
            let data_ch = data_ch.take();
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
                    if let Some(data_ch) = data_ch.as_ref() {
                        data_ch.send(result.rtt)?;
                    }
                    if logger.is_some() {
                        logger.as_mut().unwrap().log(&result).await?;
                        if recv_counter % 100 == 0 {
                            println!("RTT: {}", stats);
                            println!("PIAT: {}", stat_piat);
                        }
                    } else if log {
                        println!(
                            "{} bytes from {}: tcp_pay_seq={} time={:.3} ms \
                             retrans={}",
                            result.size,
                            result.src_addr,
                            result.seq,
                            result.rtt,
                            result.retrans
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

            while let Some(Ok(frame)) = rx.next().await {
                let recv_timestamp =
                    std::time::Duration::from(nix::time::clock_gettime(
                        nix::time::ClockId::CLOCK_MONOTONIC,
                    )?)
                    .as_nanos() as u128;
                let decoded_packet: TcpEchoPacket =
                    bincode::deserialize(&frame)?;
                // Get the current socket stats
                let stats = raw_socket.get_stats()?;

                let send_timestamp = decoded_packet.send_timestamp;
                let seq = decoded_packet.seq;
                let rtt = ((recv_timestamp - send_timestamp) as f64) / 1e6;

                result.seq = seq;
                result.rtt = rtt;
                result.send_timestamp = send_timestamp;
                result.recv_timestamp = recv_timestamp;
                result.server_timestamp = decoded_packet.recv_timestamp;
                result.size = frame.len() as usize;
                result.retrans = stats.tcpi_total_retrans as u32;

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
struct TcpEchoPacket {
    identifier: u16,
    send_timestamp: u128,
    recv_timestamp: u128,
    seq: u128,
    #[serde(with = "serde_bytes")]
    payload: Vec<u8>,
}
pub struct TCPServer {
    socket: TokioTcpListener,
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

        let mut socket = TCPSocket::new(
            None,
            Some((dst_addr, dst_port)),
            args.mss,
            args.cc,
        )?;
        socket.listen(128)?;
        let socket = TokioTcpListener::from_std(
            socket.get_ref().try_clone()?.try_into()?,
        )?;
        println!("{:?}", socket);

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
        loop {
            println!("Waiting for connection");
            if let Ok((stream, addr)) = self.socket.accept().await {
                println!("Got connection from {}", addr);
                tokio::spawn(async move {
                    Self::client_handler(stream).await.unwrap();
                })
                .await?;
            }
        }
    }

    pub async fn client_handler(mut stream: TokioTcpStream) -> Result<()> {
        let (rx, tx) = stream.split();
        let mut rx = FramedRead::new(rx, LengthDelimitedCodec::new());
        let mut tx = FramedWrite::new(tx, LengthDelimitedCodec::new());
        let mut stats = Statistics::new();
        let mut recv_counter = 0;
        let mut time = Instant::now();

        loop {
            tokio::select! {
                Some(Ok(frame)) = rx.next() => {
                if recv_counter == 0 {
                    time = Instant::now();
                } else {
                    stats.update(time.elapsed().as_millis() as f64);
                    time = Instant::now();
                }
                let mut decoded_packet: TcpEchoPacket =
                    bincode::deserialize(&frame).unwrap();
                let recv_timestamp =
                    SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
                                    as u128;
                decoded_packet.recv_timestamp = recv_timestamp;
                let encoded_packet = bincode::serialize(&decoded_packet)?;
                tx.send(encoded_packet.into()).await?;
                if (recv_counter % 100) == 0 {
                    println!("{}", stats);

                }
                recv_counter += 1;
                },
                else => {
                    println!("Client disconnected");
                    break;
                }
            }
        }
        Ok(())
    }
}
