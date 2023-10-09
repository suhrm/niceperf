#![feature(async_fn_in_trait)]
#![feature(associated_type_defaults)]

use std::{
    marker::PhantomData,
    net::SocketAddr,
    time::{Duration, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
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
    pub fn new(local: &str) -> Self {
        let local: SocketAddr = local.parse().unwrap();

        let socket =
            common::UDPSocket::new(None, Some((local.ip(), local.port())))
                .unwrap();

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
    pub fn new(remote: &str) -> Self {
        let remote = remote.parse().unwrap();
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
                    let conn = connecting.await?;
                    let (tx, rx) = conn.open_bi().await?;
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
        let client =
            TestRunner::new(Side::Server, tx, rx, stop_rx, 1000, 1000, 1000);
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
    ctx: Vec<ConnCtx>,
    timeout: u64,
    interval: u64,
    packet_size: u64,
    tx_ctrl: quinn::SendStream,
    rx_ctrl: quinn::RecvStream,
    stop: tokio::sync::oneshot::Receiver<()>,
    id: u64,
    side: Side,
    state: PhantomData<State>,
}

impl TestRunner<NotReady> {
    fn new(
        side: Side,
        tx_ctrl: quinn::SendStream,
        rx_ctrl: quinn::RecvStream,
        stop: tokio::sync::oneshot::Receiver<()>,
        timeout: u64,
        interval: u64,
        packet_size: u64,
    ) -> Self {
        Self {
            ctx: Vec::new(),
            timeout,
            interval,
            packet_size,
            tx_ctrl,
            rx_ctrl,
            stop,
            id: 0,
            side,
            state: PhantomData,
        }
    }

    async fn setup(mut self) -> Result<TestRunner<Ready>> {
        // Do the test setup things
        match self.side {
            Side::Client => {
                // Send the NewTest request
                // Wait for the NewTest response
                // Startup the ClientSide Runner
            }
            Side::Server => {
                // Wait for the NewTest request
                // Startup the ServerSide Runner and reply with the NewTest
                // response
            }
        }

        for _ in 0..self.ctx.len() {
            match self.side {
                Side::Client => {
                    self.handshake(
                        std::time::Duration::from_millis(10000),
                        Some(std::time::Duration::from_millis(100)),
                    )
                    .await?
                }
                Side::Server => {
                    self.handshake(
                        std::time::Duration::from_millis(10000),
                        None,
                    )
                    .await?
                }
            }
        }

        Ok(TestRunner::<Ready> {
            ctx: self.ctx,
            timeout: self.timeout,
            interval: self.interval,
            packet_size: self.packet_size,
            tx_ctrl: self.tx_ctrl,
            rx_ctrl: self.rx_ctrl,
            stop: self.stop,
            id: self.id,
            side: self.side,
            state: PhantomData,
        })
    }

    async fn handshake(
        &mut self,
        timeout: Duration,
        interval: Option<Duration>,
    ) -> Result<()> {
        let handshake = protocol::MessageType::Handshake(self.id as u64);

        match self.side {
            Side::Client => {
                let mut handshake_timer =
                    tokio::time::interval(interval.unwrap());
                loop {
                    tokio::select! {
                        _ = handshake_timer.tick() => {
                            let msg = bincode::serialize(&handshake).unwrap();
                            for ctx in self.ctx.iter_mut() {
                                ctx.bidi.write_all(&msg).await.unwrap();
                            }
                        },

                        _ = tokio::time::sleep(timeout) => {
                            return Err(anyhow!("handshake timeout"));
                        }
                    }
                }
            }
            Side::Server => {
                let mut recvbuf = [0u8; u16::MAX as usize];
                loop {
                    tokio::select! {
                        Ok(Some(len)) = self.rx_ctrl.read(&mut recvbuf) => {
                            let recvbuf = recvbuf[..len].to_vec();
                            let msg: protocol::MessageType = bincode::deserialize(&recvbuf).unwrap();
                            match msg {
                                protocol::MessageType::Handshake(id) => {
                                    self.id = id;
                                    break;
                                }
                                _ => {
                                    return Err(anyhow!("unexpected message type"));
                                }
                            }
                        },
                        _ = tokio::time::sleep(timeout) => {
                            return Err(anyhow!("handshake timeout"));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl TestRunner<Ready> {
    async fn run(mut self) {
        let mut snd_timer =
            tokio::time::interval(Duration::from_millis(self.interval));
        let packet_size = self.packet_size;
        assert!(packet_size <= u16::MAX as u64);
        let mut sndbuf = [0u8; u16::MAX as usize];
        let mut recvbuf = [0u8; u16::MAX as usize];

        loop {
            tokio::select! {
                _ = snd_timer.tick() => {
                    let sndbuf = sndbuf[..packet_size as usize].to_vec();
                    for ctx in self.ctx.iter_mut() {
                        ctx.bidi.write_all(&sndbuf).await.unwrap();
                    }
                }
                Ok(Some(len)) = self.rx_ctrl.read(&mut recvbuf) => {
                    let recvbuf = recvbuf[..len].to_vec();
                    self.handle_msg(&recvbuf).await;
                }

                _ = tokio::time::sleep(Duration::from_millis(self.timeout)) => {
                    for ctx in self.ctx.iter_mut() {
                        ctx.stop.take().unwrap().send(()).unwrap();
                    }
                    break;
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
impl<State> TestRunner<State> {
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
    bidi: tokio::io::DuplexStream,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    state: ConnState,
}

impl ConnCtx {
    fn new(
        bidi: tokio::io::DuplexStream,
        stop: tokio::sync::oneshot::Sender<()>,
    ) -> Self {
        Self {
            bidi,
            stop: Some(stop),
            state: ConnState::Connected,
        }
    }
}

struct ConnRunner<T: Latency> {
    bidi: tokio::io::DuplexStream,
    stop: tokio::sync::oneshot::Receiver<()>,
    socket: T,
}

impl<T: Latency> ConnRunner<T> {
    fn new(
        socket: T,
        bidi: tokio::io::DuplexStream,
        stop: tokio::sync::oneshot::Receiver<()>,
    ) -> Self {
        Self { bidi, socket, stop }
    }

    async fn run(&mut self) {
        let mut sndbuf = [0u8; u16::MAX as usize];
        let mut recvbuf = [0u8; u16::MAX as usize];

        loop {
            tokio::select! {
                len = self.bidi.read(&mut sndbuf) => {
                    let sndsndbuf = sndbuf[..len.unwrap()].to_vec();
                    self.socket.send(&sndsndbuf).await.unwrap();
                }
                len = self.socket.recv(&mut recvbuf) => {
                    let recvbuf = recvbuf[..len.unwrap()].to_vec();
                    self.bidi.write_all(&recvbuf).await.unwrap();
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
