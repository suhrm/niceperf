#![feature(async_fn_in_trait)]
mod args;
mod tcp;
mod traits;
use anyhow::Result;
use clap::Parser;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::traits::{Latency, LatencyHandler};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let _args = args::Opts::parse();
    let tcp_latency = tcp::TcpLatency::new(("0.0.0.0".parse()?, 0)).await?;
    server(tcp_latency.into()).await?;
    Ok(())
}

pub async fn server(mut proto: traits::LatencyProtocol) -> Result<()> {
    proto.run().await?;

    loop {
        tokio::select! {
            client = proto.accept() => {
                tokio::spawn(async move {
                    let mut client = client?;
                    client.read().await?;
                    client.write().await?;
                    Ok::<(), anyhow::Error>(())
                });
            }
        }
    }

    Ok(())
}
