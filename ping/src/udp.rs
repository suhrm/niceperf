use std::{
    collections::HashMap,
    net::IpAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, Logger, Statistics, UDPSocket};
use serde::{Deserialize, Serialize};
use tokio::{net::UdpSocket as tokioUdpSocket, signal};

use crate::{args, logger::UDPEchoResult};

pub struct UDPClient {
    socket: tokio::net::UdpSocket,
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
        let mut payload = UdpEchoPacket {
            identifier: self.identifier,
            seq: 0,
            send_timestamp: 0,
            recv_timestamp: 0,
            payload: vec![0u8; self.common.len.unwrap() as usize],
        };
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
                    // Build UDP echo packet
                    payload.seq = self.internal_couter;
                    payload.send_timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
                    let echo_packet = bincode::serialize(&payload)?;
                    self.socket.send_to(&echo_packet, (self.dst_addr, self.dst_port)).await?;
                    self.internal_couter += 1;
                },
                Ok((len, _recv_addr)) = self.socket.recv_from(&mut buf) => {
                    // Deserialize the packet
                    let recv_packet: UdpEchoPacket = bincode::deserialize(&buf[..len])?;

                    // Puplate the result

                    let recv_timestamp= SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
                    let rtt = ((recv_timestamp - recv_packet.send_timestamp) as f64)/1e6;
                    let result = UDPEchoResult {
                        recv_timestamp,

                        seq: recv_packet.seq,
                        send_timestamp: recv_packet.send_timestamp,
                        server_timestamp: recv_packet.recv_timestamp,
                        rtt: rtt,
                        size: len,
                        src_addr: self.src_addr.to_string(),
                        dst_addr: self.dst_addr.to_string(),
                    };
                    recv_counter += 1;
                    self.rtt_stats.update(rtt);

               if self.logger.is_some() {
                   // Safety: Safe to unwrap because the file is some
                   self.logger.as_mut().unwrap().log(&result).await?;
               }
               else{
                 // Print regular ping output
                        println!("{} bytes from {}: udp_pay_seq={} time={:.3} ms", len, result.src_addr, result.seq, result.rtt);
               }
                },
                _ = recv_stop.recv() => {
                    break;
                },
                _= signal::ctrl_c() => {
                    // Print on a new line, because some terminals will print "^C" in which makes the text look ugly
                    println!("\nCtrl-C received, exiting");
                    break
                }

            }
        }
        println!("{}", self.rtt_stats);
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
        let dst_addr = args.dst_addr;
        let dst_port = args.dst_port;

        let socket = UDPSocket::new(None, Some((dst_addr, dst_port)))?;
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
        }
    }
}
