use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, Statistics, TCPSocket};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener as TokioTcpListener, TcpStream as TokioTcpStream},
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::{
    args,
    logger::{PingLogger, TCPEchoResult},
};

pub struct TCPClient {
    socket: TokioTcpStream,
    common: args::CommonOpts,
    src_addr: IpAddr,
    dst_addr: IpAddr,
    internal_couter: u128,
    identifier: u16,
    src_port: Option<u16>,
    dst_port: u16,
    logger: Option<PingLogger>,
    cc: String,
    rtt_stats: Statistics,
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
        let socket = TokioTcpStream::from_std(
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
            // Safety we have a default value
            cc: args.cc.unwrap(),
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
            IpAddr::V6(_) => {
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
        // Convert to framed readers and writers as TCP is a streaming protocol
        let (rx, tx) = self.socket.split();
        let mut f_reader = FramedRead::new(rx, LengthDelimitedCodec::new());
        let mut f_writer = FramedWrite::new(tx, LengthDelimitedCodec::new());

        loop {
            tokio::select! {
                _ = pacing_timer .tick() => {
                    if  self.common.count.is_some() && self.internal_couter >= self.common.count.unwrap() as u128 {
                        break;

                    }
                    payload.seq = self.internal_couter;
                    payload.send_timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u128;
                    let encoded_packet = bincode::serialize(&payload)?;

                    f_writer.send(encoded_packet.into()).await?;
                    self.internal_couter += 1;
                }
                Some(Ok(frame)) = f_reader.next() => {
                    let recv_timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u128;
                    let decoded_packet: TcpEchoPacket = bincode::deserialize(&frame)?;
                    let send_timestamp = decoded_packet.send_timestamp;
                    let seq = decoded_packet.seq;
                    let rtt = ((recv_timestamp - send_timestamp) as f64) / 1e6;
                    let result = TCPEchoResult {
                        seq,
                        rtt,
                        send_timestamp,
                        recv_timestamp,
                        server_timestamp: decoded_packet.recv_timestamp,
                        src_addr : src_addr.to_string(),
                        dst_addr : dst_addr.to_string(),
                        cc: self.cc.clone(),
                    };
                    self.rtt_stats.update(rtt);
                    if let Some(logger) = &mut self.logger {
                        logger.log(&result).await?;
                    }
                    else {
                        println!("{} bytes from {}: tcp_pay_seq={} time={:.3} ms", frame.len(), result.src_addr, result.seq, result.rtt);
                    }
                }
            }
        }
        println!("{}", self.rtt_stats);
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
            // let (socket, src_addr) = self.socket.accept().await?;
            // println!("Got connection from {}", src_addr);
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
        let mut f_reader = FramedRead::new(rx, LengthDelimitedCodec::new());
        let mut f_writer = FramedWrite::new(tx, LengthDelimitedCodec::new());
        loop {
            tokio::select! {
            next_frame = f_reader.next() => {
                if let Some(Ok(frame)) = next_frame {
                let recv_timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u128;
                let mut decoded_packet: TcpEchoPacket = bincode::deserialize(&frame)?;
                decoded_packet.recv_timestamp = recv_timestamp;
                let encoded_packet = bincode::serialize(&decoded_packet)?;
                f_writer.send(encoded_packet.into()).await?;

                }
                else {
                    break;
                }
            }
            }
        }
        Ok(())
    }
}
