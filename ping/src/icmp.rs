use std::{
    net::IpAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use common::{
    interface_to_ipaddr, AsyncICMPSocket, ICMPSocket, Logger, Statistics,
};
use etherparse::{IcmpEchoHeader, Icmpv4Header, Icmpv4Type};
use tokio::signal;

use crate::{args, logger::PingResult};
pub struct ICMPClient {
    /// Logger
    logger: Option<Logger<PingResult>>,
    /// Common options
    common: args::CommonOpts,
    /// ICMP socket
    socket: AsyncICMPSocket,
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
        let dst_addr = args.dst_addr;
        let socket = ICMPSocket::new(Some(iface), None)?;

        let logger = match args.common_opts.file.clone() {
            Some(file_name) => Some(Logger::new(file_name)?),
            None => None,
        };

        Ok(ICMPClient {
            socket: AsyncICMPSocket::new(socket)?,
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
        let _src_addr = match self.src_addr {
            IpAddr::V4(addr) => addr,
            IpAddr::V6(_addr) => {
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
            "interval {} ms, preload {} packets ",
            self.common.interval.unwrap(),
            self.common.preload.unwrap()
        );
        // TODO: Add support for timeout
        let _timeout_tracker =
            tokio::time::interval(std::time::Duration::from_millis(10 * 1000));
        let mut pacing_timer = tokio::time::interval(
            std::time::Duration::from_millis(self.common.interval.unwrap()),
        );
        // TODO: Generate a random payload for the ICMP packet

        // Safety: Safe to unwrap because we have a default value
        let mut payload = vec![0u8; self.common.len.unwrap() - 8];
        if payload.len() < 16 {
            Err(anyhow!("Payload is too small"))?;
        }
        // Recv counter
        let mut recv_counter = 0;
        let (send_stop, mut recv_stop) = tokio::sync::mpsc::channel(1);
        let mut timeout_started = false;

        loop {
            tokio::select! {
                _ = pacing_timer.tick() => {
                    if  self.common.count.is_some() && self.internal_couter >= self.common.count.unwrap() as u128 {
                        if (recv_counter as u128) >= self.common.count.unwrap() as u128 {
                            break;
                        }
                        else if !timeout_started {
                            println!("Timeout started waiting for {} packets", self.common.count.unwrap() - recv_counter);
                            timeout_started = true;
                            let stop = send_stop.clone();
                            tokio::spawn( async move {
                                tokio::time::sleep(std::time::Duration::from_millis(10000)).await;
                                let _ = stop.send(()).await;
                            });
                        }
                        continue;
                    }
                    // Build ICMP packet
                    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
                    // Put timestamp in the payload
                    payload[..16].copy_from_slice(&timestamp.to_be_bytes());
                    payload[16..32].copy_from_slice(&self.internal_couter.to_be_bytes());
                    let icmp_packet = match self.dst_addr {
                        IpAddr::V4(_) =>{
                    [Icmpv4Header::with_checksum(
                        Icmpv4Type::EchoRequest(IcmpEchoHeader {
                            id: self.identifier, // Identifier
                            seq: self.internal_couter as u16, // Sequence number
                        }),
                        payload.as_slice(), // Payload for checksum
                    ).to_bytes().as_slice(), payload.as_slice()].concat() // Concatenate header and payload


                        },
                        IpAddr::V6(_) => {
                            return Err(anyhow!("IPv6 is not supported yet"));
                        }
                    };

                    self.socket.send_to(&icmp_packet, &self.dst_addr).await?;
                    self.internal_couter += 1;
                },
                Ok(len) = self.socket.read(&mut buf) => {
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

                recv_counter += 1;

               let reply_payload = icmp_header.1;
               let send_time = u128::from_be_bytes(reply_payload[..16].try_into()?);
               let recv_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
               let rtt = ((recv_time - send_time) as f64)/ 1e6;
               let ttl = buf[8];
               let seq_internal = u128::from_be_bytes(reply_payload[16..32].try_into()?);
               let result = PingResult {
                   seq: reply_header.seq,
                   rtt,
                   send_time,
                   recv_time,
                   size: len - 20, // We need to subtract the IP header size
                   ttl,
                   src_addr: self.src_addr.to_string(),
                   dst_addr: self.dst_addr.to_string(),
                   unique_seq: seq_internal,
               };
               self.rtt_stats.update(rtt);
               if self.logger.is_some() {
                   // Safety: Safe to unwrap because the file is some
                   self.logger.as_mut().unwrap().log(&result).await?;
               }
               else {
                   // If no logger is specified, print the result in standard ping format
                   println!("{} bytes from {}: icmp_seq={} ttl={} time={:.3} ms", result.size, result.src_addr, result.seq, result.ttl, result.rtt);
                }



                },
                _ = recv_stop.recv() => {
                    println!("wait for 10 seconds to receive all the packets");
                    break;
                },
                _= signal::ctrl_c() => {
                    // Print on a new line, because some terminals will print "^C" in which makes the text look ugly
                    println!("\nCtrl-C received, exiting");
                    break
                }

            }
        }
        // Print the statistics
        println!("{}", self.rtt_stats);
        Ok(())
    }
}
