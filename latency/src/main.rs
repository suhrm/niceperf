#![feature(async_fn_in_trait)]
#![feature(associated_type_defaults)]

use std::{
    net::{IpAddr, SocketAddr},
};

use anyhow::{Result};
use clap::Parser;


use tokio::io::{AsyncReadExt};


mod args;
mod conn_types;
mod protocol;
mod test_runner;

use test_runner::{Side, TestRunner};


#[tokio::main(flavor = "current_thread")]
async fn main() {
    let opts = args::Opts::parse();

    match opts.mode {
        args::Modes::Server(args::Server {
            listen_addr,
            listen_port,
            data_port_range: _,
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
