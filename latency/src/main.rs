#![feature(async_fn_in_trait)]

mod args;
use std::net::{IpAddr, SocketAddrV4};

use anyhow::Result;
use clap::Parser;
use common::{QuicClient, QuicServer, TCPSocket};
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
            let mut ctrl_client = QuicClient::new(None, None, None)?;
            ctrl_client.connect((dst_addr, dst_port)).await?;
            match proto {
                args::Protocol::Tcp(_opts) => {
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

pub trait LatencyRunner {
    async fn write(&mut self, buf: &mut [u8]) -> Result<()>;
    async fn read(&mut self, buf: &mut [u8]) -> Result<()>;
}

pub trait LatencyServer {
    async fn listen(&mut self) -> Result<()>;
    async fn accept(&mut self) -> Result<()>;
}

pub trait LatencyClient {
    async fn connect(&mut self, addr: (IpAddr, u16)) -> Result<()>;
}
pub async fn run_server<T>(mut runner: T) -> Result<()>
where
    T: LatencyServer + LatencyRunner,
{
    Ok(())
}
pub async fn run_client<T>(mut runner: T) -> Result<()>
where
    T: LatencyClient + LatencyRunner,
{
    Ok(())
}

struct TcpRunner {
    socket: TCPSocket,
}

impl LatencyRunner for TcpRunner {
    async fn read(&mut self, buf: &mut [u8]) -> Result<()> {
        Ok(())
    }
    async fn write(&mut self, buf: &mut [u8]) -> Result<()> {
        Ok(())
    }
}

impl LatencyClient for TcpRunner {
    async fn connect(&mut self, addr: (IpAddr, u16)) -> Result<()> {
        Ok(())
    }
}

impl LatencyServer for TcpRunner {
    async fn listen(&mut self) -> Result<()> {
        Ok(())
    }
    async fn accept(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn test_connection() -> Result<()> {
        let mut futures = Vec::new();
        let server = QuicServer::new(("127.0.0.1".parse()?, 0))?;
        let server_addr = server.server.local_addr()?.clone();

        futures.push(tokio::spawn(async move {
            let incomming = server.server.accept();

            let connection = incomming
                .await
                .ok_or_else(|| anyhow::anyhow!("Failed to accept"))?;

            let connection = connection.await?;
            dbg!("Accepted connection");
            let bi_stream = connection.accept_bi().await?;
            let (mut _send, mut recv) = bi_stream;
            let mut buf = [0u8; 10];
            recv.read(&mut buf).await?;
            assert_eq!(buf, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
            let dgram = connection.read_datagram().await?;
            assert_eq!(dgram, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
            dbg!("Received data");
            Ok::<(), anyhow::Error>(())

        }));

        dbg!("Server started");

        let client = QuicClient::new(None, None, None)?;

        let con = client
            .connect((server_addr.ip(), server_addr.port()))
            .await?;
        con.send_datagram(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10].into())?;
        let (mut send, mut _recv) = con.open_bi().await?;
        let snd_len = send.write(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]).await?;
        assert_eq!(snd_len, 10);

        for fut in futures {
            fut.await??;
        }

        Ok(())
    }
}
