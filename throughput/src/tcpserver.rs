use anyhow::{Error, Result};
use bytes::BytesMut;
use common::new_tcp_socket;
use futures_util::{StreamExt, TryStreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::{args::TcpServerOpts, messages};

pub async fn run(options: TcpServerOpts) -> Result<()> {
    let socket = new_tcp_socket(
        options.server_common_opts.interface,
        options.server_common_opts.server_ip,
        Some(options.server_common_opts.port),
    )?;

    socket.listen(128)?;
    let listener = TcpListener::from_std(socket.into())?;

    println!("Entering run loop");
    loop {
        let (stream, _client_address) = listener.accept().await?;
        println!("Handling connection");
        handle_stream(stream).await?
    }
}

async fn handle_stream(mut stream: TcpStream) -> Result<()> {
    let (reader, writer) = stream.split();

    let mut f_reader = FramedRead::new(reader, LengthDelimitedCodec::new());
    let _f_writer = FramedWrite::new(writer, LengthDelimitedCodec::new());

    loop {
        select! {
            received = f_reader.try_next() => {
                match received {
                    Ok(received) => {
                        match received {
                            Some(packet) => {
                                handle_packet(&packet).await?
                            },
                            None => {
                                return Ok(())
                            }
                        }
                            },
                    Err(e) => {
                        return Err(Error::from(e))
                    }
                }
            }
        }
    }
}

async fn handle_packet(packet: &BytesMut) -> Result<()> {
    match bincode::deserialize(packet)? {
        messages::PacketType::SideChannel(_sch) => {
            todo!()
        }
        messages::PacketType::Throughput(tp) => {
            println!(
                "Got TP packet with conn_id: {} and length: {}",
                tp.connection_id,
                tp.payload.len()
            );
            Ok(())
        }
    }
}
