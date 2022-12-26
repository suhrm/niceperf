use tokio::net::{TcpListener, TcpStream};
use common::new_tcp_socket;
use crate::args::TcpServerOpts;
use anyhow::{Result, Error};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use futures_util::StreamExt;
use crate::messages;

pub async fn run(options: TcpServerOpts) -> Result<()> {
    let socket = new_tcp_socket(
        options.server_common_opts.interface,
        options.server_common_opts.server_ip,
        options.server_common_opts.port
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
        if let Some(received) = f_reader.next().await {
            match received {
                Ok(received) => {
                    let decoded: messages::Packet = bincode::deserialize(&received)?;

                    match decoded.packet_type {
                        messages::PacketType::SideChannel(_sch) => {
                            todo!()
                        },
                        messages::PacketType::Throughput(tp) => {
                            println!("Got TP packet, conn_id: {}, payload_size: {} bytes",
                                     decoded.connection_id, tp.payload.len())
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