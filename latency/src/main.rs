#![feature(async_fn_in_trait)]
mod args;
mod tcp;
mod traits;
use std::{collections::HashMap, net::IpAddr};

use anyhow::{anyhow, Result};
use clap::Parser;
use common::QuicClient;
use futures_util::{SinkExt, StreamExt};
use quinn::{Connection, RecvStream, SendStream};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::StreamMap;
use tokio_util::codec::{
    Framed, FramedRead, FramedWrite, LengthDelimitedCodec,
};

use crate::traits::{Latency, LatencyHandler};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = args::Opts::parse();
    match args.mode {
        args::Modes::Client {
            dst_addr,
            dst_port,
            proto,
        } => match proto {
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
            }
        },
        args::Modes::Server {
            listen_addr,
            listen_port,
        } => {
            let mut server = CtrlServer::new((listen_addr, listen_port))?;
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
        let config = bincode::serialize(config)?;
        self.tx.send(config.into()).await?;
        Ok(())
    }

    pub async fn response_handler(&mut self) -> Result<()> {
        loop {
            // tokio::select! {
            //     Some(data) = self.rx.next() => {
            //         let data = data?;
            //         let data = data.to_vec();
            //         let data: args::Response = bincode::deserialize(&data)?;
            //         match data {
            //             args::Response::Latency(latency) => {
            //                 println!("Latency: {:?}", latency);
            //             }
            //         }
            //     }
            // }
        }
    }
}

struct CtrlServer {
    conn: common::QuicServer,
    clients: HashMap<usize, quinn::Connection>,
    stream_readers:
        StreamMap<usize, FramedRead<RecvStream, LengthDelimitedCodec>>,
    sink_writers: HashMap<usize, FramedWrite<SendStream, LengthDelimitedCodec>>,
}

impl CtrlServer {
    fn new(addr: (IpAddr, u16)) -> Result<Self> {
        let conn = common::QuicServer::new(addr)?;

        Ok(Self {
            conn,
            clients: HashMap::new(),
            stream_readers: StreamMap::new(),
            sink_writers: HashMap::new(),
        })
    }

    async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                //Accept new connections
                Some(incomming) = self.conn.server.accept() => {
                        let connection = incomming.await?;
                        let (tx, rx) = connection.accept_bi().await?;
                        let stable_id = connection.stable_id();
                        self.stream_readers.insert(stable_id, FramedRead::new(rx, LengthDelimitedCodec::new()));
                        self.sink_writers.insert(stable_id, FramedWrite::new(tx, LengthDelimitedCodec::new()));
                        self.clients.insert(stable_id, connection);
                },

                // Handle incoming data
                Some(data) = self.stream_readers.next() => {
                        println!("Received: {:?}", data);
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
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use common::QuicClient;
    use futures_util::SinkExt;

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn test_server() -> Result<()> {
        let server_addr = SocketAddr::from(([127, 0, 0, 1], 4434));
        let mut server =
            CtrlServer::new((server_addr.ip(), server_addr.port()))?;
        let fut = tokio::spawn(async move {
            server.run().await?;

            Ok::<(), anyhow::Error>(())
        });
        let client = QuicClient::new(None, None, None)?;

        let con = client
            .connect((server_addr.ip(), server_addr.port()))
            .await?;
        let (send, mut _recv) = con.open_bi().await?;
        let mut framed_sender =
            FramedWrite::new(send, LengthDelimitedCodec::new());
        framed_sender
            .send(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10].into())
            .await?;
        framed_sender
            .send(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10].into())
            .await?;

        fut.await??;
        Ok(())
    }
}
