#![feature(async_fn_in_trait)]
#![feature(associated_type_defaults)]
mod args;
mod tcp;
mod traits;
mod utils;
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
use futures_util::{FutureExt, SinkExt, StreamExt};
use quinn::{Connection, RecvStream, SendStream};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::StreamMap;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::protocol::{
    ClientMessage, ClientRequest, ServerReply, ServerResponse,
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
        let config_msg = bincode::serialize(&protocol::ClientMessage::NewTest(
            self.id.clone() as u64,
            config.clone(),
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
                                    dbg!(id);
                                    self.handshake_handler.insert(id, tx);
                                    dbg!(&self.id);
                                    println!("New test: {:?}", config);
                                    self.test_from_config(config, rx).await.unwrap();
                                }
                                protocol::ServerReply::Handshake(id) => {

                                    let id = id as usize;
                                    dbg!(id);
                                    let tx = self.handshake_handler
                                        .remove(&id).expect("id not found");
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
                let u = UdpLatency::<Unconnected>::new(
                    (opts.dst_addr, opts.dst_port),
                    0,
                )
                .unwrap();
                tokio::spawn(async move {
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

struct ClientCtx {
    stop_signal: Option<tokio::sync::oneshot::Sender<()>>,
    connection: quinn::Connection,
    fut: tokio::task::JoinHandle<()>,
}

struct CtrlServer {
    conn: common::QuicServer,
    clients: HashMap<usize, ClientCtx>,
    port_range: Option<Vec<u16>>,
}

impl CtrlServer {
    fn new(addr: (IpAddr, u16), port_range: Option<Vec<u16>>) -> Result<Self> {
        let conn = common::QuicServer::new(addr).unwrap();
        Ok(Self {
            conn,
            clients: HashMap::new(),
            port_range,
        })
    }

    async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! (
                //Accept new connections
                Some(incomming) = self.conn.server.accept() => {
                    self.handle_incomming_connection(incomming).await.unwrap();
                },

                // Handle user interrupt
                _ = tokio::signal::ctrl_c() => {
                    println!("Ctrl-C received, shutting down");
                    for (_, ctx) in &mut self.clients {
                        ctx.stop_signal.take().expect("Already stopped?!").send(()).unwrap();
                        ctx.connection.close(0u32.into(), b"Server closed by user");
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
        let rx = FramedRead::new(rx, LengthDelimitedCodec::new());
        let tx = FramedWrite::new(tx, LengthDelimitedCodec::new());

        let stop_signal = tokio::sync::oneshot::channel();

        let mut client = ClientHandle::new(stable_id, tx, rx, stop_signal.1);

        let client_ctx = ClientCtx {
            stop_signal: Some(stop_signal.0),
            connection,
            fut: tokio::spawn(async move {
                client.run().await.unwrap();
            }),
        };

        self.clients.insert(stable_id, client_ctx);

        Ok(())
    }
}

type CtrlWriter = FramedWrite<SendStream, LengthDelimitedCodec>;
type CtrlReader = FramedRead<RecvStream, LengthDelimitedCodec>;
type ClientId = usize;
type TxPacing = tokio::sync::broadcast::Sender<LatencyMsg>;

pub struct ClientHandle {
    client_id: ClientId,
    ctrl_writer: CtrlWriter,
    ctrl_reader: CtrlReader,
    stop_signal: tokio::sync::oneshot::Receiver<()>,
    tx_pacing: tokio::sync::broadcast::Sender<()>,
    test_handles: Vec<TestHandle>,
}

impl ClientHandle {
    pub fn new(
        client_id: ClientId,
        ctrl_writer: CtrlWriter,
        ctrl_reader: CtrlReader,
        stop_signal: tokio::sync::oneshot::Receiver<()>,
    ) -> Self {
        Self {
            client_id,
            ctrl_writer,
            ctrl_reader,
            stop_signal,
            tx_pacing: tokio::sync::broadcast::channel(1).0,
            test_handles: Vec::new(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut pending_handshakes =
            futures_util::stream::select_all(Vec::new());
        loop {
            tokio::select! {
                Some(msg) = self.ctrl_reader.next() => {
                    let parsed_msg = bincode::deserialize::<protocol::ClientMessage>(&msg.unwrap()).unwrap();
                    match parsed_msg {
                        protocol::ClientMessage::NewTest(id, config) => {
                            let hs = self.test_from_config(id as usize, config.clone()).await.unwrap();

                            let test_response = protocol::ServerResponse::Ok(
                                protocol::ServerReply::NewTest(id as u64, config)

                            );
                            let test_response = bincode::serialize(&test_response).unwrap();
                            self.ctrl_writer.send(test_response.into()).await.unwrap();

                            pending_handshakes.push(hs.into_stream());
                            // pending_handshakes.((id as usize, hs));
                        },

                        _ => {
                            panic!("Unexpected message from client");
                        }
                    }
                },
                Some(res) = pending_handshakes.next() => {

                    let handshake_msg = protocol::ServerResponse::Ok(
                        protocol::ServerReply::Handshake(0)
                    );
                    let handshake_msg = bincode::serialize(&handshake_msg).unwrap();
                    self.ctrl_writer.send(handshake_msg.into()).await.unwrap();
                },

                _ = &mut self.stop_signal => {
                    break Ok(());
                }
            }
        }
    }

    pub async fn test_from_config(
        &mut self,
        id: ClientId,
        conf: args::Config,
    ) -> Result<(tokio::sync::oneshot::Receiver<()>)> {
        let (pending_handshake) = match conf.proto {
            args::ProtocolType::Tcp(_) => todo!(),
            args::ProtocolType::Udp(opts) => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let u = UdpLatency::<Server>::new(
                    (opts.dst_addr, opts.dst_port),
                    0,
                )
                .unwrap();
                let tx_pacing = self.tx_pacing.subscribe();
                let (send_stop_signal, recv_stop_signal) =
                    tokio::sync::oneshot::channel();
                self.test_handles.push(TestHandle {
                    id,
                    stop_signal: send_stop_signal,
                });
                tokio::spawn(async move {
                    let mut u = handshake_handler(u, tx).await.unwrap();
                    u.run(recv_stop_signal, tx_pacing).await.unwrap();
                });
                Ok(rx)
            }

            args::ProtocolType::Quic(_) => todo!(),
        };
        pending_handshake
    }
}

pub struct TestHandle {
    id: usize,
    stop_signal: tokio::sync::oneshot::Sender<()>,
}

async fn handshake_handler<T: UnconnectedServer>(
    proto: T,
    handshake_success: HandshakeSuccessServer,
) -> Result<T::C> {
    let proto = proto.handshake(handshake_success, Timeout(10000)).await?;
    Ok(proto)
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LatencyHeader {
    id: u64,
    seq: u64,
    send_time: u64,
    recv_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LatencyMsg {
    #[serde(flatten)]
    header: LatencyHeader,
    payload: Vec<u8>,
}

pub struct Unconnected {}
use common::Logging;
#[derive(Debug, Clone, Serialize, Deserialize, Default, Logging)]
pub struct UdpLatencyResult {
    id: u64,
    seq: u64,
    send_time: u64,
    recv_time: u64,
    pkt_size: u64,
}

impl From<LatencyMsg> for UdpLatencyResult {
    fn from(msg: LatencyMsg) -> Self {
        Self {
            id: msg.header.id,
            seq: msg.header.seq,
            send_time: msg.header.send_time,
            recv_time: msg.header.recv_time,
            pkt_size: msg.payload.len() as u64
                + std::mem::size_of::<LatencyHeader>() as u64,
        }
    }
}

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

type HandshakeSuccessServer = tokio::sync::oneshot::Sender<()>;

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
        complete: HandshakeSuccessServer,
        timeout: Timeout,
    ) -> Result<Self::C> {
        let timeout = std::time::Duration::from_millis(timeout.into());

        let mut buf = [0u8; 1024];

        loop {
            println!("Waiting for handshake");
            tokio::select! {
                Ok((len, recv_addr)) = self.socket.recv_from(&mut buf) => {
                    let msg = bincode::deserialize(&buf[..len]).unwrap();

                    match msg {
                        protocol::ClientMessage::Handshake(handshake) => {
                            dbg!(&handshake);
                            if handshake.id == self.id as u64 {
                                complete.send(()).unwrap();
                            dbg ! ( "Handshake complete" ) ;
                            return Ok(UdpLatency {
                                state: std::marker::PhantomData::<Connected>,
                                logger: self.logger,
                                socket: self.socket,
                                id: self.id,
                                addr: self.addr,
                            });

                            }
                            else {
                                println!("Handshake failed");
                                dbg!(&handshake);
                                continue;
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
        complete: HandshakeSuccessServer,
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
                    println!("Sending handshake");
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

pub trait ConnectedClient: Send + Sync + Sized {
    type Msg: Default
        + std::fmt::Debug
        + std::fmt::Display
        + Logging
        + From<LatencyMsg>;
    async fn send(&mut self, latency_msg: &Self::Msg) -> Result<()>;
    async fn recv(&mut self, buf: &mut [u8]) -> Result<Self::Msg>;
}

pub trait Runner {
    async fn run(
        &mut self,
        mut stop_signal: tokio::sync::oneshot::Receiver<()>,
        mut send_signal: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()>
    where
        Self: Sized + ConnectedClient + Logger<Self::Msg>,
    {
        let mut latency_msg = Self::Msg::default();
        let mut buf = [0u8; 1500];
        println!("Starting runner");
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
        let msg: LatencyMsg = bincode::deserialize(&buf[..len]).unwrap();
        let msg = msg.into();
        Ok(msg)
    }
}
impl Logger<UdpLatencyResult> for UdpLatency<Connected> {
    #[inline]
    fn logger(&mut self) -> &mut common::Logger<UdpLatencyResult> {
        &mut self.logger
    }
}

impl Runner for UdpLatency<Connected> {
}
use common;
use polling;
use quinn_proto;
mod test {
    use std::{net::SocketAddr, sync::Arc};

    use bytes::{Bytes, BytesMut};

    use super::*;
    #[test]
    fn test() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn quinn_test() {
        let now = || std::time::Instant::now();
        let client_addr = "127.0.0.1:12346".parse().unwrap();
        let server_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let mut client_config = common::configure_client(None).unwrap();
        let mut server_config = common::configure_server().unwrap();

        let client_ep_cfg = quinn_proto::EndpointConfig::default();
        let server_ep_cfg = quinn_proto::EndpointConfig::default();

        let mut client_ep =
            quinn_proto::Endpoint::new(Arc::new(client_ep_cfg), None, false);
        let mut server_ep = quinn_proto::Endpoint::new(
            Arc::new(server_ep_cfg),
            Some(Arc::new(server_config.0)),
            true,
        );

        let (conn_handle, mut conn) = client_ep
            .connect(
                client_config,
                "127.0.0.1:12345".parse().unwrap(),
                "localhost",
            )
            .unwrap();
        let transmits = conn.poll_transmit(now(), 100).unwrap();
        dbg!(&transmits);

        let recv_data = BytesMut::from(transmits.contents.clone().as_ref());
        let (ser_con_handle, dgram_event) = server_ep
            .handle(now(), client_addr, Some(server_addr.ip()), None, recv_data)
            .unwrap();
        match dgram_event {
            quinn_proto::DatagramEvent::ConnectionEvent(_) => {
                print!("connection event");
                todo!()
            }
            quinn_proto::DatagramEvent::NewConnection(_) => {
                println!("New connection");
                todo!()
            }
        }
    }
}
