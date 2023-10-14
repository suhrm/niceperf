use std::marker::PhantomData;
use std::time::Duration;
use anyhow::{anyhow, Result};
use crate::protocol;

use crate::{
    args,
    conn_types::{socket_kind, ConnCtx, ConnRunner, UdpLatency},
};

pub enum Side {
    Client,
    Server,
}

pub struct Ready;
pub struct NotReady;

pub struct TestRunner<State = NotReady> {
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
    pub fn new(
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

    pub async fn setup(mut self) -> Result<TestRunner<Ready>> {
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
                                let socket = UdpLatency::<
                                    socket_kind::Client,
                                >::new(
                                    (
                                    udpopts.dst_addr,
                                    udpopts.dst_port,
                                )
                                );
                                let mut conn_runner =
                                    ConnRunner::new(
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
                            self.ctx.as_ref().unwrap().send(&msg).unwrap();
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
                        Ok(msg) = self.ctx.as_mut().unwrap().recv() => {
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
        assert!(self.ctx.is_some());
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
    pub async fn run(mut self) {
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
                    self.ctx.as_ref().unwrap().send(&sndbuf).unwrap();
                }
                Ok(Some(len)) = self.rx_ctrl.read(&mut recvbuf) => {
                    let recvbuf = recvbuf[..len].to_vec();
                    self.handle_msg(&recvbuf).await;
                }
                Ok(msg) = self.ctx.as_mut().unwrap().recv() => {
                    println!("Received {} bytes", msg.len());
                }

                _ = &mut self.stop => {
                    for ctx in self.ctx.iter_mut() {
                        ctx.stop();
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
