#![feature(async_fn_in_trait)]
#![feature(associated_type_defaults)]
mod args;
mod tcp;
mod traits;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    os::fd::FromRawFd,
    time,
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
async fn main() {
    let args = args::Opts::parse();
    match args.mode {
        args::Modes::Client(client_args) => {
            client_app(client_args).await.unwrap();
        }
        args::Modes::Server(server_args) => {
            server_app(server_args).await.unwrap();
        }
    }
}

async fn server_app(args: args::Server) -> Result<()> {
    let mut server = CtrlServer::new(
        (args.listen_addr, args.listen_port),
        args.data_port_range,
    )
    .unwrap();
    server.run().await.unwrap();
    Ok(())
}

async fn client_app(args: args::Client) -> Result<()> {
    let config = match args.config_type {
        args::ConfigType::File { path } => {
            let config_file = std::fs::File::open(path).unwrap();
            let config: args::Config =
                serde_json::from_reader(config_file).unwrap();
            args::Config {
                common_opts: config.common_opts,
                proto: config.proto,
            }
        }
        args::ConfigType::Protocol { proto, common_opts } => {
            args::Config { common_opts, proto }
        }
    };

    let mut client = CtrlClient::new(
        (args.dst_addr, args.dst_port),
        None, // TODO: is this needed.unwrap()
        args.cert_path.as_deref(),
    )
    .await
    .unwrap();
    client.send_config(&config).await.unwrap();

    client.response_handler().await.unwrap();
    Ok(())
}

struct CtrlClient {
    conn: common::QuicClient,
    connection: quinn::Connection,
    tx: FramedWrite<SendStream, LengthDelimitedCodec>,
    rx: FramedRead<RecvStream, LengthDelimitedCodec>,
    handshake_handler: HashMap<usize, tokio::sync::oneshot::Sender<()>>,
    id: usize,
}

impl CtrlClient {
    pub async fn new(
        addr: (IpAddr, u16),
        iface: Option<&str>,
        cert_path: Option<&str>,
    ) -> Result<Self> {
        let conn = common::QuicClient::new(iface, None, cert_path).unwrap();
        let connection = conn.connect(addr).await.unwrap();
        let (tx, rx) = connection.open_bi().await.unwrap();
        let tx = FramedWrite::new(tx, LengthDelimitedCodec::new());
        let rx = FramedRead::new(rx, LengthDelimitedCodec::new());
        let id = connection.stable_id();
        Ok(Self {
            conn,
            connection,
            tx,
            rx,
            handshake_handler: HashMap::new(),
            id,
        })
    }

    pub async fn send_config(&mut self, config: &args::Config) -> Result<()> {
        let config_msg = bincode::serialize(&protocol::ClientMessage::Request(
            protocol::ClientRequest::NewTest(
                self.id.clone() as u64,
                config.clone(),
            ),
        ))
        .unwrap();
        self.tx.send(config_msg.into()).await.unwrap();
        Ok(())
    }

    pub async fn response_handler(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                Some(Ok(data)) = self.rx.next() => {

                    let response = bincode::deserialize(&data).unwrap();
                    match response {
                        protocol::ServerResponse::Ok(reply) => {

                            match reply {
                                protocol::ServerReply::NewTest(id, config) => {
                                    let (tx, rx) = tokio::sync::oneshot::channel();
                                    let id = id as usize;
                                    self.handshake_handler.insert(id, tx);
                                    dbg!(&self.id);
                                    self.test_from_config(config, rx).await.unwrap();
                                }
                                protocol::ServerReply::Handshake(id) => {

                                    let id = id as usize;
                                    dbg!(id);
                                    let tx = self.handshake_handler.remove(&id).expect("id not found");
                                    tx.send(()).unwrap();
                                }
                            }
                        }
                        protocol::ServerResponse::Error(err) => {
                            panic!("Server error: {:?}", err);
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
    async fn test_from_config(
        &mut self,
        conf: args::Config,
        rx: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<()> {
        match conf.proto {
            args::ProtocolType::Tcp(_) => todo!(),
            args::ProtocolType::Udp(opts) => {
                tokio::spawn(async move {
                    let u = UdpLatency::<Unconnected>::new(
                        (opts.dst_addr, opts.dst_port),
                        0,
                    )
                    .unwrap();
                    // let (tx, rx) = tokio::sync::oneshot::channel();
                    let u = u
                        .handshake(rx, Timeout(10000), Interval(1000))
                        .await
                        .unwrap();
                });
                Ok(())
            }

            args::ProtocolType::Quic(_) => todo!(),
        }
    }
}

struct RunnerInfo {
    fut: tokio::task::JoinHandle<()>,
}

struct CtrlServer {
    conn: common::QuicServer,
    clients: HashMap<usize, quinn::Connection>,
    stream_readers:
        StreamMap<usize, FramedRead<RecvStream, LengthDelimitedCodec>>,
    sink_writers: HashMap<usize, FramedWrite<SendStream, LengthDelimitedCodec>>,
    port_range: Option<Vec<u16>>,
    // Used to send async messages to the main loop for async tasks that don't
    runners: HashMap<usize, RunnerInfo>,
    // finish immediately
    async_msg_tx:
        tokio::sync::mpsc::Receiver<(usize, protocol::ServerResponse)>,
    async_msg_rx: tokio::sync::mpsc::Sender<(usize, protocol::ServerResponse)>,
}

impl CtrlServer {
    fn new(addr: (IpAddr, u16), port_range: Option<Vec<u16>>) -> Result<Self> {
        let conn = common::QuicServer::new(addr).unwrap();
        let (async_msg_rx, async_msg_tx) = tokio::sync::mpsc::channel(100);
        Ok(Self {
            conn,
            clients: HashMap::new(),
            stream_readers: StreamMap::new(),
            sink_writers: HashMap::new(),
            port_range,
            runners: HashMap::new(),
            async_msg_rx,
            async_msg_tx,
        })
    }

    async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! (
                //Accept new connections
                Some(incomming) = self.conn.server.accept() => {
                    self.handle_incomming_connection(incomming).await.unwrap();
                },
                // Handle incoming data
                Some((client_id, Ok(data))) = self.stream_readers.next() => {

                    let msg = bincode::deserialize(&data);
                    match msg {
                        Ok(client_msg) => {
                            self.handle_client_msg(client_id, client_msg).await.unwrap();
                        },
                        Err(err) => {
                            self.handle_error(client_id, err.into()).await.unwrap();
                        },

                    }
                },

                // Handle async messages
                Some((id, msg)) = self.async_msg_tx.recv() => {
                    dbg!("Sending response to client ", &msg);
                    self.send_response(id, msg).await.unwrap();
                },

                // Handle user interrupt
                _ = tokio::signal::ctrl_c() => {
                    println!("Ctrl-C received, shutting down");
                    for (_, tx) in self.sink_writers.iter_mut() {
                        tx.close().await.unwrap();
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
        let connection = incomming.await.unwrap();
        let (tx, rx) = connection.accept_bi().await.unwrap();
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
            .ok_or_else(|| anyhow!("Client {} not found", client_id))
            .unwrap()
            .send(bincode::serialize(&err).unwrap().into())
            .await
            .unwrap();
        Ok(())
    }

    async fn handle_client_msg(
        &mut self,
        client_id: usize,
        msg: protocol::ClientMessage,
    ) -> Result<()> {
        match msg {
            protocol::ClientMessage::Request(request) => {
                match request {
                    protocol::ClientRequest::NewTest(id, config) => {
                        match config.proto.clone() {
                            args::ProtocolType::Tcp(_) => todo!(),
                            args::ProtocolType::Udp(opts) => {
                                let async_rx = self.async_msg_rx.clone();
                                tokio::spawn(async move {
                                    let (tx, rx) =
                                        tokio::sync::oneshot::channel::<
                                            Result<()>,
                                        >(
                                        );
                                    let mut server = UdpLatency::<Server>::new(
                                        (opts.dst_addr, opts.dst_port),
                                        client_id.clone(),
                                    )
                                    .unwrap();

                                    let u = tokio::spawn(async move {
                                        let u = server
                                            .handshake(tx, Timeout(10000))
                                            .await
                                            .unwrap();
                                        u
                                    })
                                    .await
                                    .unwrap();

                                    rx.await.unwrap();

                                    // self.runners.insert(client_id.clone(),
                                    // RunnerInfo{}
                                    //     fut: tokio::spawn(async move {
                                    //         u.run().await.unwrap();
                                    //     });
                                    //
                                    // );
                                    let response = protocol::ServerResponse::Ok(
                                        protocol::ServerReply::Handshake(
                                            id.clone(),
                                        ),
                                    );
                                    async_rx
                                        .send((client_id, response))
                                        .await
                                        .unwrap();
                                });

                                let response = protocol::ServerResponse::Ok(
                                    protocol::ServerReply::NewTest(
                                        id,
                                        config.clone(),
                                    ),
                                );
                                self.async_msg_rx
                                    .send((client_id, response))
                                    .await
                                    .unwrap();
                            }
                            args::ProtocolType::Quic(_) => todo!(),
                        }
                    }
                }
            }
            protocol::ClientMessage::Response(response) => {
                todo!();
            }
            protocol::ClientMessage::Handshake(_) => todo!(),
        };

        Ok(())
    }
    async fn send_response(
        &mut self,
        client_id: usize,
        response: protocol::ServerResponse,
    ) -> Result<()> {
        self.sink_writers
            .get_mut(&client_id)
            .ok_or_else(|| anyhow!("Client not found"))
            .unwrap()
            .send(bincode::serialize(&response).unwrap().into())
            .await
            .unwrap();
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
pub struct UdpLatency<State> {
    state: std::marker::PhantomData<State>,
    pub logger: common::Logger<UdpLatencyResult>,
    socket: tokio::net::UdpSocket,
    id: usize,
    addr: (IpAddr, u16),
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
struct Server {}

impl UnconnectedServer for UdpLatency<Server> {
    type C = UdpLatency<Connected>;
    type U = UdpLatency<Server>;
    fn new(addr: (IpAddr, u16), id: usize) -> Result<Self::U> {
        let logger = common::Logger::new("test2".into()).unwrap();

        let socket = common::UDPSocket::new(None, Some(addr)).unwrap();
        let socket = tokio::net::UdpSocket::from_std(
            socket.get_ref().try_clone().unwrap().try_into().unwrap(),
        )
        .unwrap();

        Ok(Self {
            state: std::marker::PhantomData::<Server>,
            logger,
            socket,
            id,
            addr,
        })
    }
    async fn handshake(
        self,
        complete: tokio::sync::oneshot::Sender<Result<()>>,
        timeout: Timeout,
    ) -> Result<Self::C> {
        let timeout = std::time::Duration::from_millis(timeout.into());

        let mut buf = [0u8; 1024];

        loop {
            tokio::select! {
                Ok((len, recv_addr)) = self.socket.recv_from(&mut buf) => {
                    let msg = bincode::deserialize(&buf[..len]).unwrap();

                    match msg {
                        protocol::ClientMessage::Handshake(handshake) => {
                            if handshake.id == 0 as u64 {
                                complete.send(Ok(())).unwrap();
                            dbg ! ( "Handshake complete" ) ;
                            return Ok(UdpLatency {
                                state: std::marker::PhantomData::<Connected>,
                                logger: self.logger,
                                socket: self.socket,
                                id: self.id,
                                addr: self.addr,
                            });

                            }
                        }
                        _ => {
                            continue;
                        }
                    }
                },
                _ = tokio::time::sleep(timeout) => {
                    return Err(anyhow!("Handshake timeout"));
                }
            }
        }
    }
}

trait UnconnectedServer {
    type C;
    type U;
    fn new(addr: (IpAddr, u16), id: usize) -> Result<Self::U>;
    async fn handshake(
        self,
        complete: tokio::sync::oneshot::Sender<Result<()>>,
        timeout: Timeout,
    ) -> Result<Self::C>;
}

impl UnconnectedClient for UdpLatency<Unconnected> {
    type C = UdpLatency<Connected>;
    type U = UdpLatency<Unconnected>;
    // TODO: Should be created based on received config
    fn new(addr: (IpAddr, u16), id: usize) -> Result<Self::U> {
        let logger = common::Logger::new("test".into()).unwrap();

        let socket = common::UDPSocket::new(None, None).unwrap();
        let socket = tokio::net::UdpSocket::from_std(
            socket.get_ref().try_clone().unwrap().try_into()?,
        )
        .unwrap();

        Ok(Self {
            state: std::marker::PhantomData::<Unconnected>,
            logger,
            socket,
            id,
            addr,
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
                    let msg = bincode::serialize(&handshake).unwrap();
                    self.socket.send_to( &msg, self.addr).await.unwrap();
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
            addr: self.addr,
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
                   self.send(&latency_msg).await.unwrap();
                },
                Ok(recv_msg) = self.recv(&mut buf) => {
                    self.logger().log(&recv_msg).await.unwrap();
                }
                _ = &mut stop_signal => {
                    break Ok(());
                }
            }
        }
    }
}

pub trait Logger<T>
where
    T: Logging + std::fmt::Display,
{
    fn logger(&mut self) -> &mut common::Logger<T>;
}

impl ConnectedClient for UdpLatency<Connected> {
    type Msg = UdpLatencyResult;
    async fn send(&mut self, latency_msg: &Self::Msg) -> Result<()> {
        let msg = bincode::serialize(latency_msg).unwrap();
        self.socket.send_to(&msg, self.addr).await.unwrap();
        Ok(())
    }
    async fn recv(&mut self, buf: &mut [u8]) -> Result<Self::Msg> {
        let (len, recv_addr) = self.socket.recv_from(buf).await.unwrap();
        if (recv_addr.ip(), recv_addr.port()) != self.addr {
            return Err(anyhow!("Received from unknown address"));
        }
        let msg: UdpLatencyResult = bincode::deserialize(&buf[..len]).unwrap();
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
        let logger = common::Logger::new("test".into()).unwrap();
        let mut socket =
            common::TCPSocket::new(None, None, None, None).unwrap();
        socket.connect(addr.into()).unwrap();
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
            self.raw_socket.get_ref().try_clone().unwrap().try_into()?,
        )
        .unwrap();

        loop {
            tokio::select! {
                _ = &mut complete => {
                    break;
                },
                _ = handshake_timer.tick() => {
                    let msg = bincode::serialize(&handshake).unwrap();
                    socket.write_all(&msg).await.unwrap();
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
        let msg = bincode::serialize(latency_msg).unwrap();
        self.socket
            .as_mut()
            .ok_or(anyhow!("Not connected"))
            .unwrap()
            .write_all(&msg)
            .await
            .unwrap();
        Ok(())
    }
    async fn recv(&mut self, buf: &mut [u8]) -> Result<Self::Msg> {
        let len = self
            .socket
            .as_mut()
            .ok_or(anyhow!("Not Connected while reading "))
            .unwrap()
            .read(buf)
            .await
            .unwrap();
        let msg: TcpLatencyResult = bincode::deserialize(&buf[..len]).unwrap();
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

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn test_ctrl_proto() -> Result<()> {
        let client_opts = args::Client {
            config_type: args::ConfigType::Protocol {
                proto: args::ProtocolType::Udp(args::UDPOpts {
                    dst_addr: "127.0.0.1".parse().unwrap(),
                    dst_port: 55555,
                }),
                common_opts: args::CommonOpts::default(),
            },
            cert_path: None,
            dst_addr: "127.0.0.1".parse().unwrap(),
            dst_port: 8080,
        };

        let server_opts = args::Server {
            listen_addr: "127.0.0.1".parse().unwrap(),
            listen_port: 8080,
            data_port_range: None,
        };

        tokio::spawn(async move {
            server_app(server_opts).await.unwrap();
            Ok::<(), anyhow::Error>(())
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        client_app(client_opts).await.unwrap();

        Ok(())
    }
}
