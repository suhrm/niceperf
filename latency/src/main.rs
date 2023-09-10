#![feature(async_fn_in_trait)]
#![feature(associated_type_defaults)]
mod args;
mod tcp;
mod traits;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    os::fd::FromRawFd,
};
mod protocol;

use anyhow::{anyhow, Result};
use clap::{arg, Parser};
use common::{QuicClient, QuicServer};
use futures_util::{SinkExt, StreamExt};
use quinn::{Connection, RecvStream, SendStream};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::StreamMap;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::{
    protocol::ServerReply,
    traits::{Latency, LatencyHandler},
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = args::Opts::parse();
    match args.mode {
        args::Modes::Client(args::Client {
            dst_addr,
            dst_port,
            proto,
        }) => {
            match proto {
                args::Protocol::Tcp(_) => todo!(),
                args::Protocol::Udp(_) => todo!(),
                args::Protocol::Quic(_) => todo!(),
                args::Protocol::File(fileopts) => {
                    let config_file = std::fs::File::open(fileopts.path)?;
                    let config: args::Config =
                        serde_json::from_reader(config_file)?;
                    let mut client = CtrlClient::new(
                        (dst_addr, dst_port),
                        config.common_opts.iface.as_deref(),
                        config.common_opts.cert_path.as_deref(),
                    )
                    .await?;
                    client.send_config(&config).await?;
                    client.response_handler().await?;
                }
            }
        }
        args::Modes::Server(args::Server {
            listen_addr,
            listen_port,
            data_port_range,
        }) => {
            let mut server =
                CtrlServer::new((listen_addr, listen_port), data_port_range)?;
            server.run().await?;
        }
    }

    Ok(())
}

struct CtrlClient {
    conn: common::QuicClient,
    connection: quinn::Connection,
    tx: FramedWrite<SendStream, LengthDelimitedCodec>,
    rx: FramedRead<RecvStream, LengthDelimitedCodec>,
}

impl CtrlClient {
    pub async fn new(
        addr: (IpAddr, u16),
        iface: Option<&str>,
        cert_path: Option<&str>,
    ) -> Result<Self> {
        let conn = common::QuicClient::new(iface, None, cert_path)?;
        let connection = conn.connect(addr).await?;
        let (tx, rx) = connection.open_bi().await?;
        let tx = FramedWrite::new(tx, LengthDelimitedCodec::new());
        let rx = FramedRead::new(rx, LengthDelimitedCodec::new());
        Ok(Self {
            conn,
            connection,
            tx,
            rx,
        })
    }

    pub async fn send_config(&mut self, config: &args::Config) -> Result<()> {
        let config_msg = bincode::serialize(
            &protocol::ClientRequest::NewTest(config.clone()),
        )?;
        self.tx.send(config_msg.into()).await?;
        Ok(())
    }

    pub async fn response_handler(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                Some(Ok(data)) = self.rx.next() => {
                    let response = bincode::deserialize(&data)?;
                    match response {
                        protocol::ServerResponse::Ok(reply) => {
                            match reply {
                                protocol::ServerReply::NewTest(_config) => {
                                    todo!();
                                }
                            }
                        }
                        protocol::ServerResponse::Error(_err) => {
                            todo!();
                        }
                    }
                },
                _ = tokio::signal::ctrl_c() => {
                    println!("Ctrl-C received, shutting down");
                    self.connection.close(0u32.into(), b"Client closed by user");
                    break Ok(());
                }
            }
        }
    }
}
struct CtrlServer {
    conn: common::QuicServer,
    clients: HashMap<usize, quinn::Connection>,
    stream_readers:
        StreamMap<usize, FramedRead<RecvStream, LengthDelimitedCodec>>,
    sink_writers: HashMap<usize, FramedWrite<SendStream, LengthDelimitedCodec>>,
    port_range: Option<Vec<u16>>,
}

impl CtrlServer {
    fn new(addr: (IpAddr, u16), port_range: Option<Vec<u16>>) -> Result<Self> {
        let conn = common::QuicServer::new(addr)?;
        Ok(Self {
            conn,
            clients: HashMap::new(),
            stream_readers: StreamMap::new(),
            sink_writers: HashMap::new(),
            port_range,
        })
    }

    async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! (
                //Accept new connections
                Some(incomming) = self.conn.server.accept() => {
                    self.handle_incomming_connection(incomming).await?;
                },
                // Handle incoming data
                Some((client_id, Ok(data))) = self.stream_readers.next() => {
                    let msg = bincode::deserialize(&data);
                    match msg {
                        Ok(client_msg) => {
                            self.handle_client_msg(client_id, client_msg).await?;
                        },
                        Err(err) => {
                            self.handle_error(client_id, err.into()).await?;
                        },
                    }
                },
                // Handle user interrupt
                _ = tokio::signal::ctrl_c() => {
                    println!("Ctrl-C received, shutting down");
                    for (_, tx) in self.sink_writers.iter_mut() {
                        tx.close().await?;
                    }

                    for (_, conn) in self.clients.iter() {
                        conn.close(0u32.into(), b"Server closed by user");
                    }
                    break Ok(());
                }
            )
        }
    }
    async fn handle_incomming_connection(
        &mut self,
        incomming: quinn::Connecting,
    ) -> Result<()> {
        let connection = incomming.await?;
        let (tx, rx) = connection.accept_bi().await?;
        let stable_id = connection.stable_id();
        self.stream_readers.insert(
            stable_id,
            FramedRead::new(rx, LengthDelimitedCodec::new()),
        );
        self.sink_writers.insert(
            stable_id,
            FramedWrite::new(tx, LengthDelimitedCodec::new()),
        );
        self.clients.insert(stable_id, connection);
        Ok(())
    }

    async fn handle_error(
        &mut self,
        client_id: usize,
        err: anyhow::Error,
    ) -> Result<()> {
        let err = format!("{:?}", err);
        let err = protocol::ServerResponse::Error(protocol::ServerError {
            code: 1, // TODO: define error codes
            message: err.clone().into(),
        });
        self.sink_writers
            .get_mut(&client_id)
            .ok_or_else(|| anyhow!("Client {} not found", client_id))?
            .send(bincode::serialize(&err)?.into())
            .await?;
        Ok(())
    }

    async fn handle_client_msg(
        &mut self,
        client_id: usize,
        msg: protocol::ClientMessage,
    ) -> Result<()> {
        let response = match msg {
            protocol::ClientMessage::Request(request) => {
                match request {
                    protocol::ClientRequest::NewTest(config) => {
                        match config.proto {
                            args::Protocol::Tcp(_) => todo!(),
                            args::Protocol::Udp(_) => todo!(),
                            args::Protocol::Quic(_) => todo!(),
                            args::Protocol::File(_) => todo!(),
                        }
                    }
                }
            }
            protocol::ClientMessage::Response(response) => {
                todo!();
            }
            protocol::ClientMessage::Handshake(_) => todo!(),
        };
        self.sink_writers
            .get_mut(&client_id)
            .ok_or_else(|| anyhow!("Client not found"))?
            .send(bincode::serialize(&response)?.into())
            .await?;
        Ok(())
    }
}

pub struct Unconnected {}
use common::Logging;
#[derive(Debug, Clone, Serialize, Deserialize, Default, Logging)]
pub struct UdpLatencyResult {}
#[derive(Debug, Clone, Serialize, Deserialize, Default, Logging)]
pub struct TcpLatencyResult {}
#[non_exhaustive]
pub struct UdpLatency<State = Unconnected> {
    state: std::marker::PhantomData<State>,
    pub logger: common::Logger<UdpLatencyResult>,
    socket: tokio::net::UdpSocket,
    id: usize,
}

pub struct Connected {}

#[derive(Debug, Clone)]
pub struct Timeout(u64);

impl From<u64> for Timeout {
    fn from(val: u64) -> Self {
        Self(val)
    }
}
impl Into<u64> for Timeout {
    fn into(self) -> u64 {
        self.0
    }
}
#[derive(Debug, Clone)]
pub struct Interval(u64);

impl From<u64> for Interval {
    fn from(val: u64) -> Self {
        Self(val)
    }
}
impl Into<u64> for Interval {
    fn into(self) -> u64 {
        self.0
    }
}

impl UnconnectedClient for UdpLatency<Unconnected> {
    type C = UdpLatency<Connected>;
    type U = UdpLatency<Unconnected>;
    // TODO: Should be created based on received config
    fn new(addr: (IpAddr, u16), id: usize) -> Result<Self::U> {
        let logger = common::Logger::new("test".into())?;

        let socket = common::UDPSocket::new(None, None)?;
        socket.connect(addr)?;
        let socket = tokio::net::UdpSocket::from_std(
            socket.get_ref().try_clone()?.try_into()?,
        )?;

        Ok(Self {
            state: std::marker::PhantomData::<Unconnected>,
            logger,
            socket,
            id,
        })
    }
    async fn handshake(
        self,
        mut complete: tokio::sync::oneshot::Receiver<()>,
        timeout: Timeout,
        interval: Interval,
    ) -> Result<Self::C> {
        let handshake =
            protocol::ClientMessage::Handshake(protocol::ClientHandshake {
                id: self.id as u64,
                protocol: protocol::TestType::Udp as u64,
            });
        let interval = std::time::Duration::from_millis(interval.into());
        let timeout = std::time::Duration::from_millis(timeout.into());
        let mut handshake_timer = tokio::time::interval(interval);

        loop {
            tokio::select! {
                _ = &mut complete => {
                    break;
                },
                _ = handshake_timer.tick() => {
                    let msg = bincode::serialize(&handshake)?;
                    self.socket.send(&msg).await?;
                },
                _ = tokio::time::sleep(timeout) => {
                    return Err(anyhow!("Handshake timeout"));
                }
            }
        }
        Ok(UdpLatency {
            state: std::marker::PhantomData::<Connected>,
            logger: self.logger,
            socket: self.socket,
            id: self.id,
        })
    }
}

pub trait UnconnectedClient {
    type C = Connected;
    type U = Unconnected;
    fn new(addr: (IpAddr, u16), id: usize) -> Result<Self::U>;
    async fn handshake(
        self,
        complete: tokio::sync::oneshot::Receiver<()>,
        timeout: Timeout,
        interval: Interval,
    ) -> Result<Self::C>;
}

pub trait ConnectedClient {
    type Msg;
    async fn send(&mut self, latency_msg: &Self::Msg) -> Result<()>;
    async fn recv(&mut self, buf: &mut [u8]) -> Result<Self::Msg>;
}

pub trait Runner<T> {
    type Msg = T;
    async fn run(
        &mut self,
        mut stop_signal: tokio::sync::oneshot::Receiver<()>,
        mut send_signal: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()>
    where
        T: Default + Logging + std::fmt::Display,
        Self: Sized + Logger<T> + ConnectedClient<Msg = T>,
    {
        let latency_msg = T::default();
        let mut buf = [0u8; 1500];
        loop {
            tokio::select! {
                _ = send_signal.recv() => {
                   self.send(&latency_msg).await?;
                },
                Ok(recv_msg) = self.recv(&mut buf) => {
                    self.logger().log(&recv_msg).await?;
                }
                _ = &mut stop_signal => {
                    break Ok(());
                }
            }
        }
    }
}

pub trait Logger<T> {
    fn logger(&mut self) -> &mut common::Logger<T>;
}

impl ConnectedClient for UdpLatency<Connected> {
    type Msg = UdpLatencyResult;
    async fn send(&mut self, latency_msg: &Self::Msg) -> Result<()> {
        let msg = bincode::serialize(latency_msg)?;
        self.socket.send(&msg).await?;
        Ok(())
    }
    async fn recv(&mut self, buf: &mut [u8]) -> Result<Self::Msg> {
        let len = self.socket.recv(buf).await?;
        let msg: UdpLatencyResult = bincode::deserialize(&buf[..len])?;
        Ok(msg)
    }
}
impl Logger<UdpLatencyResult> for UdpLatency<Connected> {
    #[inline]
    fn logger(&mut self) -> &mut common::Logger<UdpLatencyResult> {
        &mut self.logger
    }
}

impl Runner<UdpLatencyResult> for UdpLatency<Connected> {
}

// TODO: Find out if we want to use this method or the specific one above

pub struct TcpLatency<State = Unconnected> {
    state: std::marker::PhantomData<State>,
    logger: common::Logger<TcpLatencyResult>,
    socket: Option<tokio::net::TcpStream>,
    raw_socket: common::TCPSocket,
    id: usize,
}

impl UnconnectedClient for TcpLatency<Unconnected> {
    type C = TcpLatency<Connected>;
    type U = TcpLatency<Unconnected>;

    fn new(addr: (IpAddr, u16), id: usize) -> Result<Self::U> {
        let logger = common::Logger::new("test".into())?;
        let mut socket = common::TCPSocket::new(None, None, None, None)?;
        socket.connect(addr.into())?;
        Ok(Self {
            state: std::marker::PhantomData::<Unconnected>,
            logger,
            socket: None,
            raw_socket: socket,
            id,
        })
    }
    async fn handshake(
        self,
        mut complete: tokio::sync::oneshot::Receiver<()>,
        timeout: Timeout,
        interval: Interval,
    ) -> Result<Self::C> {
        let handshake =
            protocol::ClientMessage::Handshake(protocol::ClientHandshake {
                id: self.id as u64,
                protocol: protocol::TestType::Tcp as u64,
            });
        let interval = std::time::Duration::from_millis(interval.into());
        let timeout = std::time::Duration::from_millis(timeout.into());
        let mut handshake_timer = tokio::time::interval(interval);
        let mut socket = tokio::net::TcpStream::from_std(
            self.raw_socket.get_ref().try_clone()?.try_into()?,
        )?;

        loop {
            tokio::select! {
                _ = &mut complete => {
                    break;
                },
                _ = handshake_timer.tick() => {
                    let msg = bincode::serialize(&handshake)?;
                    socket.write_all(&msg).await?;
                },

                _ = tokio::time::sleep(timeout) => {
                    return Err(anyhow!("Handshake timeout"));
                }
            }
        }
        Ok(TcpLatency {
            state: std::marker::PhantomData::<Connected>,
            logger: self.logger,
            socket: Some(socket),
            raw_socket: self.raw_socket,
            id: self.id,
        })
    }
}

impl ConnectedClient for TcpLatency<Connected> {
    type Msg = TcpLatencyResult;
    async fn send(&mut self, latency_msg: &Self::Msg) -> Result<()> {
        let msg = bincode::serialize(latency_msg)?;
        self.socket
            .as_mut()
            .ok_or(anyhow!("Not connected"))?
            .write_all(&msg)
            .await?;
        Ok(())
    }
    async fn recv(&mut self, buf: &mut [u8]) -> Result<Self::Msg> {
        let len = self
            .socket
            .as_mut()
            .ok_or(anyhow!("Not Connected while reading "))?
            .read(buf)
            .await?;
        let msg: TcpLatencyResult = bincode::deserialize(&buf[..len])?;
        Ok(msg)
    }
}

impl Logger<TcpLatencyResult> for TcpLatency<Connected> {
    #[inline]
    fn logger(&mut self) -> &mut common::Logger<TcpLatencyResult> {
        &mut self.logger
    }
}

impl Runner<TcpLatencyResult> for TcpLatency<Connected> {
}

pub struct QuicLatency {}

