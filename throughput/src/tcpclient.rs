use std::net::SocketAddr;
use anyhow::{Result, Error};
use tokio::net::TcpStream;
use common::new_tcp_socket;
use crate::args::TcpClientOpts;
use crate::messages::{PacketType, Throughput};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use futures::sink::SinkExt;
use crate::messages;

pub async fn run(options: TcpClientOpts) -> Result<()> {
    let socket = new_tcp_socket(
        options.client_common_opts.interface,
        options.client_common_opts.client_ip,
        options.client_common_opts.client_port
    )?;

    let server_address = SocketAddr::new(
        options.client_common_opts.server_ip,
        options.client_common_opts.server_port);
    socket.set_nonblocking(false)?;
    socket.connect(&server_address.into())?;
    socket.set_nonblocking(true)?;
    let mut client = TcpStream::from_std(socket.into())?;

    let (reader, writer) = client.split();

    let _f_reader = FramedRead::new(reader, LengthDelimitedCodec::new());
    let mut f_writer = FramedWrite::new(writer, LengthDelimitedCodec::new());


    let tp_packet = PacketType::Throughput(
        Throughput {
            connection_id: 0,
            payload: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        }
    );

    let encoded = bincode::serialize(&tp_packet)?;

    f_writer.send(bytes::Bytes::from(encoded)).await?;
    //client.write_all(&encoded).await?;

    Ok(())
}