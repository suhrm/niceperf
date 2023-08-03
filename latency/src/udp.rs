use std::net::IpAddr;

use anyhow::Result;
use tokio::net::{
    TcpListener as TokioTcpListener, TcpStream as TokioTcpStream,
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::traits::{Latency, LatencyHandler, LatencyRW};

pub struct UdpLatency {
    listener: TokioTcpListener,
}

impl UdpLatency {
    pub async fn new(addr: (IpAddr, u16)) -> Result<Self> {
        Ok(Self {
            listener: TokioTcpListener::bind(addr).await?,
        })
    }
}
impl Latency for UdpLatency {
    async fn run(&mut self) -> Result<()> {
        println!("Running TCP Latency Test");
        Ok(())
    }
    async fn accept(&mut self) -> Result<LatencyRW> {
        let (stream, _) = self.listener.accept().await?;
        let stream = TcpClient::new(stream).await?;
        Ok(stream.into())
    }
}
pub struct TcpClient {
    reader: FramedRead<tokio::net::tcp::OwnedReadHalf, LengthDelimitedCodec>,
    writer: FramedWrite<tokio::net::tcp::OwnedWriteHalf, LengthDelimitedCodec>,
}

impl TcpClient {
    pub async fn new(stream: TokioTcpStream) -> Result<Self> {
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: FramedRead::new(reader, LengthDelimitedCodec::new()),
            writer: FramedWrite::new(writer, LengthDelimitedCodec::new()),
        })
    }
}
impl LatencyHandler for TcpClient {
    async fn write(&mut self) -> Result<()> {
        Ok(())
    }
    async fn read(&mut self) -> Result<()> {
        Ok(())
    }
}
