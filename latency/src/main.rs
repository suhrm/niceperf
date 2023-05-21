mod args;
use std::{
    net::{IpAddr, SocketAddrV4},
    ops::DerefMut,
    pin::Pin,
};

use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use common::{QuicClient, QuicServer, TCPSocket};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener as TokioTcpListener, TcpStream as TokioTcpStream},
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use uuid::Uuid;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = args::Opts::parse();

    match args.mode {
        args::Modes::Client {
            proto,
            dst_addr,
            dst_port,
        } => {
            let ctrl_client = QuicClient::new(None, None, None)?;
            ctrl_client.connect((dst_addr, dst_port)).await?;
            match proto {
                args::Protocol::Tcp(_opts) => {
                    let mut runner = TcpRunner::new(_opts)?;
                    run_client(runner).await?;
                    todo!();
                }
                args::Protocol::Udp(_opts) => {
                    todo!();
                }
                args::Protocol::Quic(_opts) => {
                    todo!();
                }
            }
        }
        args::Modes::Server { .. } => {
            todo!();
        }
    }

    Ok(())
}

struct CrtlClient {
    quic_client: common::QuicClient,
    id: Uuid,
    test_params: TestParams,
}
// This should basically be a struct that holds all the options for the test
// and is passed to the server based on the protocol
struct TestParams {
    test_type: args::Modes,
}

impl CrtlClient {
    pub fn new(test_params: TestParams) -> Result<Self> {
        Ok(Self {
            quic_client: common::QuicClient::new(None, None, None)?,
            id: Uuid::new_v4(),
            test_params,
        })
    }

    pub fn configure(&mut self) -> Result<()> {
        Ok(())
    }
}
#[async_trait]
pub trait LatencyRunner {
    async fn write(&mut self, buf: &mut [u8]) -> Result<()>;
    async fn read(&mut self, buf: &mut [u8]) -> Result<()>;
}

#[async_trait]
pub trait LatencyServer {
    async fn listen(&mut self) -> Result<()>;
    async fn accept(&mut self) -> Result<(Box<dyn LatencyRunner>)>;
}

#[async_trait]
pub trait LatencyClient {
    async fn connect(&mut self, addr: (IpAddr, u16)) -> Result<()>;
}

pub async fn run_server<T>(mut runner: T) -> Result<()>
where
    T: LatencyServer + LatencyRunner,
{
    let mut clients = Vec::new();
    loop {
        tokio::select! {
            Ok(_) = runner.listen() => {
                let client = runner.accept().await?;
                clients.push(client);
            },
            _ = tokio::signal::ctrl_c() => {
                break;
            }
        }

    }

    Ok(())
}

pub async fn run_client<T>(mut runner: T) -> Result<()>
where
    T: LatencyClient + LatencyRunner + Send + Sync,
{
    let mut pacing_timer =
        tokio::time::interval(std::time::Duration::from_millis(100));
    runner.connect(("127.0.0.1".parse()?, 0)).await?;
    loop {
        let mut rbuf = [0u8; 1];
        let mut wbuf = [0u8; 1];
        tokio::select! {
             _ = pacing_timer.tick() =>{
                runner.write(&mut wbuf).await?;
                todo!();
            },
            Ok(_) = runner.read(&mut wbuf) => {
                todo!();
            }
        }
    }

    Ok(())
}
struct TcpRunner {
    socket: TCPSocket, // Base socket for the runner
    // Usage depends on the mode i.e. client or server
    lstn_socket: Option<TokioTcpListener>,
    conn_socket: Option<TokioTcpStream>,
    args: args::TCPOpts,
}

impl TcpRunner {
    /// This will only create the underlying socket for the runners listen and
    /// conn will determine the usage
    pub fn new(args: args::TCPOpts) -> Result<TcpRunner> {
        let mut socket =
            TCPSocket::new(None, None, args.mss.clone(), args.cc.clone())?;

        Ok(Self {
            socket,
            lstn_socket: None,
            conn_socket: None,
            args,
        })
    }
}

#[async_trait]
impl LatencyRunner for TcpRunner {
    async fn read(&mut self, buf: &mut [u8]) -> Result<()> {
        self.conn_socket
            .as_mut()
            .ok_or(anyhow::anyhow!("No connection socket"))?
            .read_exact(buf)
            .await?;
        Ok(())
    }
    async fn write(&mut self, buf: &mut [u8]) -> Result<()> {
        self.conn_socket
            .as_mut()
            .ok_or(anyhow::anyhow!("No connection socket"))?
            .write_all(buf)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl LatencyClient for TcpRunner {
    async fn connect(&mut self, addr: (IpAddr, u16)) -> Result<()> {
        self.socket
            .get_mut()
            .connect(&std::net::SocketAddr::from(addr).into())?;
        let socket = TokioTcpStream::from_std(
            self.socket.get_ref().try_clone()?.try_into()?,
        )?;
        self.conn_socket = Some(socket);
        Ok(())
    }
}

#[async_trait]
impl LatencyServer for TcpRunner {
    async fn listen(&mut self) -> Result<()> {
        let backlog = 32;
        self.socket.get_mut().listen(backlog)?;
        let socket = TokioTcpListener::from_std(
            self.socket.get_ref().try_clone()?.try_into()?,
        )?;
        self.lstn_socket = Some(socket);
        Ok(())
    }
    async fn accept(&mut self) -> Result<(Box<dyn LatencyRunner>)> {
        let (socket, _) = self
            .lstn_socket
            .as_mut()
            .ok_or(anyhow::anyhow!("No listener socket"))?
            .accept()
            .await?;
        Ok(Box::new(socket))
    }
}
#[async_trait]
impl LatencyRunner for TokioTcpStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<()> {
        self.read_exact(buf).await?;
        Ok(())
    }
    async fn write(&mut self, buf: &mut [u8]) -> Result<()> {
        self.write_all(buf).await?;
        Ok(())
    }
}
