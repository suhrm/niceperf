use std::net::IpAddr;
use anyhow::Result;

#[derive(Debug)]
#[derive(Debug)]
pub struct ConnCtx {
    tx: tokio::sync::broadcast::Sender<Vec<u8>>,
    rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
}

impl ConnCtx {
    pub fn new(
        tx: tokio::sync::broadcast::Sender<Vec<u8>>,
        rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
        stop: tokio::sync::oneshot::Sender<()>,
    ) -> Self {
        Self {
            tx,
            rx,
            stop: Some(stop),
        }
    }

    pub fn send(&self, buf: &[u8]) -> Result<()> {
        self.tx.send(buf.to_vec())?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<Vec<u8>> {
        if let Some(buf) = self.rx.recv().await {
            return Ok(buf);
        } else {
            return Err(anyhow::anyhow!("recv error"));
        }
    }
    pub fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.send(()).unwrap();
        }
    }
}

pub struct ConnRunner<T: Latency> {
    tx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    rx: tokio::sync::mpsc::Sender<Vec<u8>>,
    stop: tokio::sync::oneshot::Receiver<()>,
    socket: T,
}

impl<T: Latency> ConnRunner<T> {
    pub fn new(
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

    pub async fn run(&mut self) {
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
pub trait Latency {
    async fn send(&mut self, buf: &[u8]) -> Result<usize>;
    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize>;
}

pub mod socket_kind {
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
        // TODO: populate rest of the options at some point.
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
