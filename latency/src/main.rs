#![feature(async_fn_in_trait)]
mod args;
mod tcp;
mod traits;
use std::collections::HashMap;

use anyhow::{anyhow, Result};
use clap::Parser;
use common::QuicClient;
use futures_util::{SinkExt, StreamExt};
use quinn::{Connection, RecvStream, SendStream};
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
        args::Modes::Client { dst_addr, dst_port } => {
            let client = QuicClient::new(None, None, None)?;

            let con = client.connect((dst_addr, dst_port)).await?;
            let (mut send, mut recv) = con.open_bi().await?;
            dbg!("Opened stream");
            let mut framed_sender =
                FramedWrite::new(send, LengthDelimitedCodec::new());
            let snd_len = framed_sender
                .send(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10].into())
                .await?;
        }
        args::Modes::Server {
            listen_addr,
            listen_port,
        } => {
            let mut server = CtrlServer::new((
                listen_addr.to_string().as_str(),
                listen_port,
            ))?;
            server.run().await?;
        }
    }

    Ok(())
}

struct CtrlClient {
    conn: common::QuicClient,
}

struct CtrlServer {
    conn: common::QuicServer,
}

type CtrlStream = (usize, (quinn::SendStream, quinn::RecvStream));

impl CtrlServer {
    fn new(addr: (&str, u16)) -> Result<Self> {
        let conn = common::QuicServer::new((addr.0.parse()?, addr.1))?;

        Ok(Self { conn })
    }

    async fn run(&mut self) -> Result<()> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);

        let mut stream_readers = StreamMap::new();
        let mut sink_writers = HashMap::new();

        loop {
            tokio::select! {
                incomming = self.conn.server.accept() => {
                    if let Some(connecting) = incomming {
                        let connection = connecting.await?;
                        (|connection: quinn::Connection,
                        tx: tokio::sync::mpsc::Sender<CtrlStream>| {
                        tokio::spawn(async move  {
                            let stream = connection.accept_bi().await?;
                            tx.send((connection.stable_id(), stream)).await?;
                            Ok::<(), anyhow::Error>(())
                        });
                        })(connection, tx.clone());

                    }
                },
                Some(streams) = rx.recv() => {
                        stream_readers.insert(streams.0, FramedRead::new(streams.1.1, LengthDelimitedCodec::new()));
                        sink_writers.insert(streams.0, FramedWrite::new(streams.1.0, LengthDelimitedCodec::new()));
                        dbg!("New stream");

                },
                Some(data) = stream_readers.next() => {
                        println!("Received: {:?}", data);
                },


                _ = tokio::signal::ctrl_c() => {
                    println!("Ctrl-C received, shutting down");
                    break Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, sync::Arc};

    use common::QuicClient;
    use futures_util::SinkExt;
    use tokio::sync::Mutex;

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn test_server() -> Result<()> {
        let mut futures = Vec::new();
        let server_addr = SocketAddr::from(([127, 0, 0, 1], 4434));
        let mut server = CtrlServer::new(("127.0.0.1", 4434))?;
        futures.push(tokio::spawn(async move {
            server.run().await?;

            Ok::<(), anyhow::Error>(())
        }));
        let client = QuicClient::new(None, None, None)?;

        let con = client
            .connect((server_addr.ip(), server_addr.port()))
            .await?;
        let (mut send, mut recv) = con.open_bi().await?;
        let mut framed_sender =
            FramedWrite::new(send, LengthDelimitedCodec::new());
        let snd_len = framed_sender
            .send(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10].into())
            .await?;

        for fut in futures {
            fut.await??;
        }
        Ok(())
    }
}
