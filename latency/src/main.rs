#![feature(async_fn_in_trait)]
#![feature(associated_type_defaults)]

use std::{
    marker::PhantomData,
    net::{IpAddr, SocketAddr},
    time::{Duration, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use clap::Parser;
use common::Logging;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
mod args;
mod protocol;
mod tcp;
mod traits;
mod utils;
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let opts = args::Opts::parse();

    match opts.mode {
        args::Modes::Server(args::Server {
            listen_addr,
            listen_port,
            data_port_range,
        }) => {
            let mut server = CtrlServer::new((listen_addr, listen_port).into());
            server.run().await.unwrap();
        }

        args::Modes::Client(args) => client(args).await,
    }
}

async fn client(args: args::Client) {
    let config = match args.config_type {
        args::ConfigType::File { path } => {
            let config_file = std::fs::File::open(path).unwrap();
            let config: args::Config =
                serde_json::from_reader(config_file).unwrap();
            args::Config {
                proto: config.proto,
                common_opts: config.common_opts,
            }
        }
        args::ConfigType::Protocol { proto, common_opts } => {
            args::Config { proto, common_opts }
        }
    };

    let mut client = CtrlClient::new(
        None,
        config.common_opts.iface.as_deref(),
        args.cert_path.as_deref(),
        (args.dst_addr, args.dst_port).into(),
    )
    .await
    .unwrap();
    client.run(config).await.unwrap();
}

struct CtrlClient(quinn::Connection);

impl CtrlClient {
    pub async fn new(
        bind_addr: Option<(IpAddr, u16)>,
        bind_iface: Option<&str>,
        cert_path: Option<&str>,
        server_addr: (IpAddr, u16),
    ) -> Result<Self> {
        let client = common::QuicClient::new(bind_iface, bind_addr, cert_path)?;
        let client = client.connect(server_addr).await?;
        Ok(Self(client))
    }

    pub async fn run(&mut self, test_cfg: args::Config) -> Result<()> {
        let (tx, rx) = self.0.open_bi().await?;

        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let id = self.0.stable_id() as u64;
        let fut = tokio::spawn(async move {
            let runner = TestRunner::new(
                Side::Client,
                tx,
                rx,
                stop_rx,
                Some(test_cfg),
                Some(id),
            );
            let runner = runner.setup().await.unwrap();
            runner.run().await;
        });

        tokio::select! {
            _ = fut => {
                println!("Test finished");
            }
            _ = tokio::signal::ctrl_c() => {
                stop_tx.send(()).unwrap();
                println!("Test stopped");
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LatencyMsg {
    id: u64,
    seq: u64,
    timestamp: u64,
    payload: Vec<u8>,
}
#[derive(Debug, Clone, Default, Logging)]
struct LatencyResult {
    id: u64,
    seq: u64,
    send_timestamp: u64,
    recv_timestamp: u64,
    payload_size: usize,
}

impl From<LatencyMsg> for LatencyResult {
    fn from(msg: LatencyMsg) -> Self {
        Self {
            id: msg.id,
            seq: msg.seq,
            send_timestamp: msg.timestamp,
            recv_timestamp: std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            payload_size: msg.payload.len() + 8 + 8 + 8,
        }
    }
}

pub trait Latency {
    async fn send(&mut self, buf: &[u8]) -> Result<usize>;
    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize>;
}

mod socket_kind {
    pub struct Server {}
    pub struct Client {}
}

pub struct UdpLatency<Kind> {
    inner: tokio::net::UdpSocket,
    peer: Option<std::net::SocketAddr>,
    kind: std::marker::PhantomData<Kind>,
}
impl UdpLatency<socket_kind::Server> {
    pub fn new(local: (IpAddr, u16)) -> Self {
        let socket = common::UDPSocket::new(None, Some(local)).unwrap();

        let inner = tokio::net::UdpSocket::from_std(
            socket.get_ref().try_clone().unwrap().into(),
        )
        .unwrap();

        Self {
            inner,
            peer: None,
            kind: std::marker::PhantomData,
        }
    }
}

impl UdpLatency<socket_kind::Client> {
    pub fn new(remote: (IpAddr, u16)) -> Self {
        let remote = remote.into();
        let socket = common::UDPSocket::new(None, None).unwrap();
        let inner = tokio::net::UdpSocket::from_std(
            socket.get_ref().try_clone().unwrap().into(),
        )
        .unwrap();

        Self {
            inner,
            peer: Some(remote),
            kind: std::marker::PhantomData,
        }
    }
}

impl<Kind> Latency for UdpLatency<Kind> {
    async fn send(&mut self, buf: &[u8]) -> Result<usize> {
        let len = self.inner.send_to(buf, self.peer.unwrap()).await?;
        Ok(len)
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        let (len, recv_addr) = self.inner.recv_from(buf).await?;
        if self.peer.is_some() {
            if recv_addr != self.peer.unwrap() {
                return Err(anyhow::anyhow!("recv from wrong addr"));
            }
        } else {
            self.peer = Some(recv_addr);
        }

        Ok(len)
    }
}

struct ClientCtx {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    fut: tokio::task::JoinHandle<()>,
}

impl ClientCtx {
    fn new(
        stop: tokio::sync::oneshot::Sender<()>,
        fut: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self {
            stop: Some(stop),
            fut,
        }
    }
}
struct CtrlServer {
    quinn: common::QuicServer,
    clients: Vec<ClientCtx>,
}
impl CtrlServer {
    fn new(lst_addr: SocketAddr) -> Self {
        let quinn =
            common::QuicServer::new((lst_addr.ip(), lst_addr.port())).unwrap();
        Self {
            quinn,
            clients: Vec::new(),
        }
    }

    async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                Some(connecting) = self.quinn.server.accept() => {
                    println!("New client connected");
                    let conn = connecting.await?;
                    let (tx, rx) = conn.accept_bi().await?;
                    println!("New stream opened");
                    self.handle_client(tx, rx).await?;
                }
            _ = tokio::signal::ctrl_c() => {
                    for client in self.clients.iter_mut() {
                        client.stop.take().unwrap().send(()).unwrap();
                    }
                    println!("Ctrl-C received, shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_client(
        &mut self,
        tx: quinn::SendStream,
        rx: quinn::RecvStream,
    ) -> Result<()> {
        let (stop, stop_rx) = tokio::sync::oneshot::channel();
        let client = TestRunner::new(Side::Server, tx, rx, stop_rx, None, None);
        let fut = tokio::spawn(async move {
            let client = client.setup().await.unwrap();
            client.run().await;
        });
        self.clients.push(ClientCtx::new(stop, fut));
        Ok(())
    }
}
pub enum Side {
    Client,
    Server,
}

struct Ready;
struct NotReady;

struct TestRunner<State = NotReady> {
    ctx: Option<ConnCtx>,
    test_cfg: Option<args::Config>,
    tx_ctrl: quinn::SendStream,
    rx_ctrl: quinn::RecvStream,
    stop: tokio::sync::oneshot::Receiver<()>,
    id: Option<u64>,
    side: Side,
    state: PhantomData<State>,
}

impl TestRunner<NotReady> {
    fn new(
        side: Side,
        tx_ctrl: quinn::SendStream,
        rx_ctrl: quinn::RecvStream,
        stop: tokio::sync::oneshot::Receiver<()>,
        test_cfg: Option<args::Config>,
        id: Option<u64>,
    ) -> Self {
        Self {
            ctx: None,
            tx_ctrl,
            rx_ctrl,
            stop,
            id,
            side,
            test_cfg,
            state: PhantomData,
        }
    }

    async fn setup(mut self) -> Result<TestRunner<Ready>> {
        // Do the test setup things
        println!("Setting up test");
        match self.side {
            Side::Client => {
                let new_test_msg = protocol::MessageType::NewTest(
                    self.id.unwrap(),
                    self.test_cfg.clone().unwrap(),
                );

                let enc_msg = bincode::serialize(&new_test_msg).unwrap();

                self.tx_ctrl.write_all(&enc_msg).await.unwrap();
                println!("Sent NewTest message");

                let mut recvbuf = [0u8; 1024];
                let len =
                    self.rx_ctrl.read(&mut recvbuf).await.unwrap().unwrap();
                let msg: protocol::MessageType =
                    bincode::deserialize(&recvbuf[..len]).unwrap();
                println!("Received {:?}", &msg);
                match msg {
                    protocol::MessageType::NewTest(_id, cfg) => {
                        let proto = cfg.proto;
                        match proto {
                            args::ProtocolType::Tcp(_) => todo!(),
                            args::ProtocolType::Udp(udpopts) => {
                                let (socket_tx, _) =
                                    tokio::sync::broadcast::channel(10);
                                let (tx, socket_rx) =
                                    tokio::sync::mpsc::channel(10);

                                let (stop, stop_rx) =
                                    tokio::sync::oneshot::channel();
                                let socket =
                                    UdpLatency::<socket_kind::Client>::new((
                                        udpopts.dst_addr,
                                        udpopts.dst_port,
                                    ));
                                let mut conn_runner = ConnRunner::new(
                                    socket,
                                    socket_tx.subscribe(),
                                    tx,
                                    stop_rx,
                                );
                                tokio::spawn(async move {
                                    conn_runner.run().await;
                                });
                                let ctx =
                                    ConnCtx::new(socket_tx, socket_rx, stop);
                                self.ctx = Some(ctx);
                            }
                            args::ProtocolType::Quic(_) => todo!(),
                        }

                        Ok(())
                    }
                    protocol::MessageType::Handshake(_) => {
                        Err(anyhow::anyhow!(
                            "Handshake from server when not expecting it"
                        ))
                    }
                    protocol::MessageType::Error(_) => {
                        Err(anyhow::anyhow!("Error from server"))
                    }
                }?;
            }
            Side::Server => {
                let mut recvbuf = [0u8; 1024];
                let len =
                    self.rx_ctrl.read(&mut recvbuf).await.unwrap().unwrap();
                println!("Got {} bytes", len);
                let msg: protocol::MessageType =
                    bincode::deserialize(&recvbuf[..len]).unwrap();
                println!("Got message: {:?}", msg);
                match msg {
                    protocol::MessageType::NewTest(id, cfg) => {
                        let resp =
                            protocol::MessageType::NewTest(id, cfg.clone());
                        self.test_cfg = Some(cfg.clone());
                        let enc_msg = bincode::serialize(&resp).unwrap();
                        self.tx_ctrl.write_all(&enc_msg).await.unwrap();

                        match cfg.proto {
                            args::ProtocolType::Tcp(_) => todo!(),
                            args::ProtocolType::Udp(udpopts) => {
                                let (socket_tx, _) =
                                    tokio::sync::broadcast::channel(10);
                                let (tx, socket_rx) =
                                    tokio::sync::mpsc::channel(10);

                                let (stop, stop_rx) =
                                    tokio::sync::oneshot::channel();
                                let socket =
                                    UdpLatency::<socket_kind::Server>::new((
                                        udpopts.dst_addr,
                                        udpopts.dst_port,
                                    ));
                                let mut conn_runner = ConnRunner::new(
                                    socket,
                                    socket_tx.subscribe(),
                                    tx,
                                    stop_rx,
                                );
                                tokio::spawn(async move {
                                    conn_runner.run().await;
                                });
                                let ctx =
                                    ConnCtx::new(socket_tx, socket_rx, stop);
                                self.ctx = Some(ctx);
                            }
                            args::ProtocolType::Quic(_) => todo!(),
                        }

                        Ok(())
                    }
                    protocol::MessageType::Handshake(_) => {
                        todo!("Handshake from client when not expecting it")
                    }
                    protocol::MessageType::Error(_) => {
                        Err(anyhow::anyhow!("Error from client"))
                    }
                }?;
            }
        }

        let handshake_timeout = std::time::Duration::from_millis(10000);

        match self.side {
            Side::Client => {
                let handshake =
                    protocol::MessageType::Handshake(self.id.unwrap());
                let handshake_interval = std::time::Duration::from_millis(100);
                let mut handshake_timer =
                    tokio::time::interval(handshake_interval);
                let mut recvbuf = [0u8; 1024];
                loop {
                    tokio::select! {
                        _ = handshake_timer.tick() => {
                            let msg = bincode::serialize(&handshake).unwrap();
                            self.ctx.as_mut().unwrap().tx.send(msg).unwrap();
                        },

                        _ = tokio::time::sleep(handshake_timeout) => {
                            return Err(anyhow!("handshake timeout"));
                        }
                        Ok(Some(len)) = self.rx_ctrl.read(&mut recvbuf) => {
                            println!("Client Received {} bytes", len);
                            let recvbuf = recvbuf[..len].to_vec();
                            let msg: protocol::MessageType = bincode::deserialize(&recvbuf).unwrap();
                            match msg {
                                protocol::MessageType::Handshake(id) => {
                                    if id == self.id.unwrap() {
                                        break;
                                    }
                                }
                                _ => {
                                    return Err(anyhow!("unexpected message type"));
                                }
                            }
                        },
                    }
                }
            }
            Side::Server => {
                loop {
                    tokio::select! {
                        Some(msg) = self.ctx.as_mut().unwrap().rx.recv() => {
                            println!("Server Received {} bytes", msg.len());
                            let msg = bincode::deserialize::<protocol::MessageType>(&msg);
                            let msg = msg.unwrap();

                            match msg {
                                protocol::MessageType::Handshake(id) => {
                                    let handshake = protocol::MessageType::Handshake(id);
                                    let msg = bincode::serialize(&handshake).unwrap();
                                    self.tx_ctrl.write_all(&msg).await.unwrap();
                                    break;
                                }
                                _ => {
                                    return Err(anyhow!("unexpected message type"));
                                }

                            }
                        },
                        _ = tokio::time::sleep(handshake_timeout) => {
                            return Err(anyhow!("handshake timeout"));
                        }
                    }
                }
            }
        }
        println!("Handshake complete, starting test");
        assert!(self.test_cfg.is_some());
        Ok(TestRunner::<Ready> {
            ctx: self.ctx,
            tx_ctrl: self.tx_ctrl,
            rx_ctrl: self.rx_ctrl,
            stop: self.stop,
            id: self.id,
            side: self.side,
            test_cfg: self.test_cfg,
            state: PhantomData,
        })
    }
}

impl TestRunner<Ready> {
    async fn run(mut self) {
        println!("Starting test");
        println!("Test config: {:?}", self.test_cfg);
        let cfg = self.test_cfg.take().unwrap();
        let mut snd_timer = tokio::time::interval(Duration::from_millis(
            cfg.common_opts.interval.unwrap(),
        ));
        let packet_size = cfg.common_opts.len.unwrap();
        assert!(packet_size <= u16::MAX as usize);
        let mut sndbuf = [0u8; u16::MAX as usize];
        let mut recvbuf = [0u8; u16::MAX as usize];

        loop {
            println!("Waiting for timer");
            tokio::select! {
                _ = snd_timer.tick() => {
                    let sndbuf = sndbuf[..packet_size as usize].to_vec();
                    self.ctx.as_mut().unwrap().tx.send(sndbuf).unwrap();
                }
                Ok(Some(len)) = self.rx_ctrl.read(&mut recvbuf) => {
                    let recvbuf = recvbuf[..len].to_vec();
                    self.handle_msg(&recvbuf).await;
                }
                Some(msg) = self.ctx.as_mut().unwrap().rx.recv() => {
                    println!("Received {} bytes", msg.len());
                }

                _ = &mut self.stop => {
                    for ctx in self.ctx.iter_mut() {
                        ctx.stop.take().unwrap().send(()).unwrap();
                    }
                    break;
                }

            }
        }
    }
}
impl TestRunner<NotReady> {
    async fn handle_msg(&mut self, msg: &[u8]) -> Result<()> {
        let msg = bincode::deserialize::<protocol::MessageType>(msg).unwrap();
        match self.side {
            Side::Client => {
                match msg {
                    protocol::MessageType::Handshake(_) => Ok(()),
                    protocol::MessageType::NewTest(id, config) => {
                        todo!()
                    }
                    protocol::MessageType::Error(err) => {
                        Err(anyhow!("Got err from server"))?
                    }
                }
            }
            Side::Server => todo!(),
        }
    }
}

impl TestRunner<Ready> {
    async fn handle_msg(&mut self, msg: &[u8]) -> Result<()> {
        let msg = bincode::deserialize::<protocol::MessageType>(msg).unwrap();
        match self.side {
            Side::Client => {
                match msg {
                    protocol::MessageType::Handshake(_) => Ok(()),
                    protocol::MessageType::NewTest(id, config) => {
                        todo!()
                    }
                    protocol::MessageType::Error(err) => {
                        Err(anyhow!("Got err from server"))?
                    }
                }
            }
            Side::Server => todo!(),
        }
    }
}

#[derive(Debug)]
pub enum ConnState {
    Connected,
    Disconnected,
}

#[derive(Debug)]
struct ConnCtx {
    tx: tokio::sync::broadcast::Sender<Vec<u8>>,
    rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    state: ConnState,
}

impl ConnCtx {
    fn new(
        tx: tokio::sync::broadcast::Sender<Vec<u8>>,
        rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
        stop: tokio::sync::oneshot::Sender<()>,
    ) -> Self {
        Self {
            tx,
            rx,
            stop: Some(stop),
            state: ConnState::Connected,
        }
    }
}

struct ConnRunner<T: Latency> {
    tx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    rx: tokio::sync::mpsc::Sender<Vec<u8>>,
    stop: tokio::sync::oneshot::Receiver<()>,
    socket: T,
}

impl<T: Latency> ConnRunner<T> {
    fn new(
        socket: T,
        tx: tokio::sync::broadcast::Receiver<Vec<u8>>,
        rx: tokio::sync::mpsc::Sender<Vec<u8>>,
        stop: tokio::sync::oneshot::Receiver<()>,
    ) -> Self {
        Self {
            tx,
            rx,
            socket,
            stop,
        }
    }

    async fn run(&mut self) {
        let mut sndbuf = [0u8; u16::MAX as usize];
        let mut recvbuf = [0u8; u16::MAX as usize];

        loop {
            tokio::select! {
                 Ok(msg) = self.tx.recv() => {
                    self.socket.send(&msg).await.unwrap();
                }
                len = self.socket.recv(&mut recvbuf) => {
                    println!("Got packet");
                    let recvbuf = recvbuf[..len.unwrap()].to_vec();
                    self.rx.send(recvbuf).await.unwrap();
                }
                _ = &mut self.stop => {
                    break;
                }
            }
        }
    }
}

mod test {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test(flavor = "current_thread")]
    async fn test_crtl_msg_handling() {
    }
}
