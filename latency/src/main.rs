#![feature(async_fn_in_trait)]
mod args;
mod tcp;
mod traits;
use std::{collections::HashMap, net::IpAddr};
mod protocol;

use anyhow::{anyhow, Result};
use clap::{arg, Parser};
use common::QuicClient;
use futures_util::{SinkExt, StreamExt};
use quinn::{Connection, RecvStream, SendStream};
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
        }) => match proto {
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
        },
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
                    let response: protocol::ServerResponse = bincode::deserialize(&data)?;
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
                Some((client_id, Ok(data))) = self.stream_readers.next() => {
                    let msg = bincode::deserialize(&data);
                    match msg {
                        Ok(client_msg) => {self.handle_client_msg(client_id, client_msg).await?;},
                        Err(err) => {
                            let err =
                                format!("Unsupported protocol: {:?}", err);
                                protocol::ServerResponse::Error(
                                    protocol::ServerError {
                                        code: 1,
                                        message: err.clone().into(),
                                    },
                            );
                            let err_msg = bincode::serialize(&err)?;
                            self.sink_writers.get_mut(&client_id).unwrap().send(err_msg.into()).await?;
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
            }
        }
    }

    async fn handle_client_msg(
        &mut self,
        client_id: usize,
        msg: protocol::ClientMessage,
    ) -> Result<()> {
        let response = match msg {
            protocol::ClientMessage::Request(request) => match request {
                protocol::ClientRequest::NewTest(config) => {
                    match config.proto {
                        args::Protocol::Tcp(_) => todo!(),
                        args::Protocol::Udp(_) => todo!(),
                        args::Protocol::Quic(_) => todo!(),
                        args::Protocol::File(_) => todo!(),
                    }
                }
            },
            protocol::ClientMessage::Response(response) => {
                todo!();
            }
        };
        self.sink_writers
            .get_mut(&client_id)
            .ok_or_else(|| anyhow!("Client not found"))?
            .send(bincode::serialize(&response)?.into())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn test_new_test_request() -> Result<()> {
        let test_opts = args::Config {
            common_opts: args::CommonOpts::default(),
            proto: args::Protocol::Udp(args::UDPOpts {
                dst_port: 9999,
                dst_addr: "10.0.0.1".parse()?,
            }),
        };
        let mut server = CtrlServer::new(("127.0.0.1".parse()?, 5555), None)?;

        tokio::spawn(async move {
            server.run().await.unwrap();
        });

        let mut client =
            CtrlClient::new(("127.0.0.1".parse()?, 5555), None, None).await?;
        client.send_config(&test_opts).await?;
        client.response_handler().await?;

        Ok(())
    }
}
