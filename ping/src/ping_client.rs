#![feature(async_fn_in_trait)]
#![feature(associated_type_defaults)]
use std::{fmt::Display, net::IpAddr, time::Duration};

use anyhow::{anyhow, Result};
use common::{Logger, Logging, Statistics};
use serde::{Deserialize, Serialize};

use crate::args;

pub trait PingClient<'a>
where
    Self: Sized + PingWrite + PingRead,
{
    type PingPacket: PingPayload
        + Serialize
        + Deserialize<'a>
        + Clone
        + Logging
        + Default
        + Display;
    fn dst_addr(&self) -> IpAddr;
    fn src_addr(&self) -> IpAddr;
    fn split(
        self,
    ) -> (
        impl PingWrite<Input = Self::PingPacket>,
        impl PingRead<Output = Self::PingPacket>,
    );

    fn gen_payload(&self) -> Self::PingPacket;
    fn logger(&self) -> Option<Logger<Self::PingPacket>>;
}

trait PingPayload {
    fn update(&mut self, seq: u128, send_timestamp: u128, recv_timestamp: u128);
}

pub trait PingRead {
    type Output;
    async fn read(&mut self) -> Result<Self::Output>;
}
pub trait PingWrite {
    type Input;
    async fn write(&mut self, packet: Self::Input) -> Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Logging)]
struct MyPingPacket {
    test: u8,
}

impl PingPayload for MyPingPacket {
    fn update(
        &mut self,
        seq: u128,
        send_timestamp: u128,
        recv_timestamp: u128,
    ) {
        todo!()
    }
}

struct MyClient;

impl PingWrite for MyClient {
    type Input = MyPingPacket;

    async fn write(&mut self, packet: Self::Input) -> Result<()> {
        todo!()
    }
}

impl PingRead for MyClient {
    type Output = MyPingPacket;

    async fn read(&mut self) -> Result<Self::Output> {
        todo!()
    }
}

impl PingClient<'_> for MyClient {
    type PingPacket = MyPingPacket;

    fn dst_addr(&self) -> IpAddr {
        todo!()
    }

    fn src_addr(&self) -> IpAddr {
        todo!()
    }

    fn split(
        self,
    ) -> (
        impl PingWrite<Input = Self::PingPacket>,
        impl PingRead<Output = Self::PingPacket>,
    ) {
        todo!()
    }

    fn gen_payload(&self) -> Self::PingPacket {
        todo!()
    }

    fn logger(&self) -> Option<Logger<Self::PingPacket>> {
        todo!()
    }
}

pub async fn run(
    mut client: impl PingClient<'_>,
    common: args::CommonOpts,
) -> Result<()> {
    let dst_addr = match client.dst_addr() {
        IpAddr::V4(addr) => addr,
        IpAddr::V6(_) => {
            return Err(anyhow!("IPv6 is not supported yet"));
        }
    };
    let src_addr = match client.src_addr() {
        IpAddr::V4(addr) => addr,
        IpAddr::V6(_) => {
            return Err(anyhow!("IPv6 is not supported yet"));
        }
    };

    // Safety: Safe to unwrap because we have a default value
    println!(
        "Pinging {} with {} bytes of data",
        dst_addr,
        common.len.unwrap()
    );
    println!(
        "interval {} ms, preload {} packets ",
        common.interval.unwrap(),
        common.preload.unwrap()
    );
    // TODO: Generate a random payload for the ICMP packet
    let mut payload = client.gen_payload();
    // Convert to tokio TcpStream

    // Recv counter
    let mut recv_counter = 0;
    let mut send_seq = 0;
    let (send_stop, mut recv_stop) = tokio::sync::mpsc::channel::<()>(1);
    let interval = common.interval.unwrap();
    let num_ping = common.count.take();

    let (mut tx, mut rx) = client.split();

    let send_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut pacing_timer =
            tokio::time::interval(std::time::Duration::from_millis(interval));
        loop {
            if let Some(num_ping) = num_ping {
                if send_seq > num_ping {
                    println!("Sent {} packets", num_ping);
                    break;
                }
            }
            _ = pacing_timer.tick().await;
            payload.update(send_seq as u128, 0, 0);

            tx.write(payload).await?;
            send_seq = send_seq + 1;
        }
        Ok::<(), anyhow::Error>(())
    });

    let mut logger = client.logger().take();
    let log = common.log.take().unwrap();
    let recv_task = tokio::spawn(async move {
        let mut stats = Statistics::new();
        let mut result = client.gen_payload();
        let (log_writer, mut log_reader) =
            tokio::sync::mpsc::channel::<client::PingPacket>(10000);
        let mut time = 0;
        let mut stat_piat = Statistics::new();
        tokio::spawn(async move {
            while let Some(result) = log_reader.recv().await {
                if recv_counter == 0 {
                    time = result.recv_timestamp;
                } else {
                    let diff = (result.recv_timestamp - time) / 1000000;
                    stat_piat.update(diff as f64);
                    time = result.recv_timestamp;
                }
                stats.update(result.rtt);
                if logger.is_some() {
                    logger.as_mut().unwrap().log(&result).await?;
                    if recv_counter % 100 == 0 {
                        println!("RTT: {}", stats);
                        println!("PIAT: {}", stat_piat);
                    }
                } else if log {
                    println!(
                        "{} bytes from {}: tcp_pay_seq={} time={:.3} ms \
                         retrans={}",
                        result.size,
                        result.src_addr,
                        result.seq,
                        result.rtt,
                        result.retrans
                    );
                } else {
                    if recv_counter % 100 == 0 {
                        println!("RTT: {}", stats);
                        println!("PIAT: {}", stat_piat);
                    }
                }
                recv_counter += 1;
            }
            Ok::<(), anyhow::Error>(())
        });

        while let Ok(frame) = rx.read().await {
            let recv_timestamp = std::time::Duration::from(
                nix::time::clock_gettime(nix::time::ClockId::CLOCK_MONOTONIC)?,
            )
            .as_nanos() as u128;
            // Get the current socket stats

            log_writer.send(result.clone()).await?;
        }
        Ok::<(), anyhow::Error>(())
    });

    tokio::select! {
        _ = send_task => {
            println!("Send stop");
            if recv_counter < send_seq {
                println!("Sent {} packets, but only received {} packets", send_seq, recv_counter);
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        },
        failure = recv_task => {
            panic!("Recv task failed: {:?}", failure);

        }
    }

    Ok(())
}



