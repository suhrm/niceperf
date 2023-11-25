use std::net::IpAddr;

use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, ICMPSocket, Logger, Statistics};
use etherparse::{IcmpEchoHeader, Icmpv4Header, Icmpv4Type};

use crate::{args, logger::PingResult};
pub struct ICMPClient {
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

impl ICMPClient {
    pub fn new(args: args::ICMPOpts) -> Result<ICMPClient> {
        let iface = args
            .common_opts
            .iface
            .clone()
            .ok_or(anyhow!("No interface specified"))?;
        let iface = iface.as_str();
        let src_addr = interface_to_ipaddr(iface)?;

        let dst_addr = args.common_opts.dst_addr;
        let socket = ICMPSocket::new(Some(iface), None)?;
        socket.connect(dst_addr)?;

        let logger = match args.common_opts.file.clone() {
            Some(file_name) => Some(Logger::new(file_name)?),
            None => None,
        };

        Ok(ICMPClient {
            socket,
            common: args.common_opts,
            src_addr,
            dst_addr,
            internal_couter: 0,
            //Safety: Safe to unwrap because we have a default value
            identifier: rand::random::<u16>(),
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
        let mut payload = vec![0u8; self.common.len.unwrap() - 8];
        if payload.len() < 16 {
            Err(anyhow!("Payload is too small"))?;
        }

        // Recv counter
        let mut send_seq: u128 = 0;
        let mut recv_counter: u128 = 0;
        let interval = self.common.interval.unwrap();
        let num_ping = self.common.count.take();
        let identifier = self.identifier;
        let socket = self.socket.get_ref().try_clone()?;
        let mut tx =
            common::AsyncICMPSocket::new(ICMPSocket::from_raw_socket(socket)?)?;
        let socket = self.socket.get_ref().try_clone()?;
        let mut rx =
            common::AsyncICMPSocket::new(ICMPSocket::from_raw_socket(socket)?)?;

        let send_task = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let mut pacing_timer = tokio::time::interval(
                std::time::Duration::from_millis(interval),
            );
            loop {
                if let Some(num_ping) = num_ping {
                    if send_seq > num_ping as u128 {
                        println!("Sent {} packets", num_ping);
                        break;
                    }
                }
                _ = pacing_timer.tick().await;
                let send_timestamp =
                    std::time::Duration::from(nix::time::clock_gettime(
                        nix::time::ClockId::CLOCK_MONOTONIC,
                    )?)
                    .as_nanos() as u128;
                payload[..16].copy_from_slice(&send_timestamp.to_be_bytes());
                payload[16..32].copy_from_slice(&send_seq.to_be_bytes());
                let icmp_packet = {
                    [
                        Icmpv4Header::with_checksum(
                            Icmpv4Type::EchoRequest(IcmpEchoHeader {
                                id: identifier,       // Identifier
                                seq: send_seq as u16, // Sequence number
                            }),
                            payload.as_slice(), // Payload for checksum
                        )
                        .to_bytes()
                        .as_slice(),
                        payload.as_slice(),
                    ]
                    .concat() // Concatenate header and payload
                };

                tx.send(&icmp_packet).await?;
                send_seq = send_seq + 1;
            }
            Ok::<(), anyhow::Error>(())
        });
        let mut logger = self.logger.take();
        let log = self.common.log.take().unwrap();
        let recv_task = tokio::spawn(async move {
            let mut stats = Statistics::new();
            let mut result = PingResult {
                seq: 0,
                rtt: 0.0,
                size: 0,
                unique_seq: 0,
                ttl: 0,
                send_timestamp: 0,
                recv_timestamp: 0,
                dst_addr: dst_addr.to_string(),
                src_addr: src_addr.to_string(),
            };

            let (log_writer, mut log_reader) =
                tokio::sync::mpsc::channel::<PingResult>(10000);
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
                            "{} bytes from {}: icmp_pay_seq={} time={:.3} ms ",
                            result.size,
                            result.src_addr,
                            result.unique_seq,
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
            while let Ok(len) = rx.read(&mut buf).await {
                let recv_timestamp =
                    std::time::Duration::from(nix::time::clock_gettime(
                        nix::time::ClockId::CLOCK_MONOTONIC,
                    )?)
                    .as_nanos() as u128;
                let icmp_header = Icmpv4Header::from_slice(&buf[20..len])?;

                let reply_header = match icmp_header.0.icmp_type {
                    Icmpv4Type::EchoReply(header) => {
                        // Check if the packet is a reply to our packet
                        if header.id == identifier {
                            header
                        } else {
                            println!("Received reply, but not our packet");
                            continue;
                        }
                    }
                    _ => {
                        println!("Received non-echo reply packet");
                        continue;
                    }
                };

                recv_counter += 1;

                let reply_payload = icmp_header.1;
                result.send_timestamp =
                    u128::from_be_bytes(reply_payload[..16].try_into()?);
                let rtt =
                    ((recv_timestamp - result.send_timestamp) as f64) / 1e6;
                let ttl = buf[8];
                let seq_internal =
                    u128::from_be_bytes(reply_payload[16..32].try_into()?);

                result.seq = reply_header.seq;
                result.unique_seq = seq_internal;
                result.rtt = rtt;
                result.size = len as usize;
                result.ttl = ttl;

                log_writer.send(result.clone()).await?;
            }
            Ok::<(), anyhow::Error>(())
        });
        tokio::select! {
            _ = send_task => {
                println!("Send stop");
                if recv_counter < send_seq {
                    println!("Sent {} packets, but only received {} packets", send_seq, recv_counter);
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                }
            },
            failure = recv_task => {
                panic!("Recv task failed: {:?}", failure);

            }
        }
        // Print the statistics
        Ok(())
    }
}
