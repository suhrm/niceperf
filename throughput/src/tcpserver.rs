use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use common::new_tcp_socket;
use crate::args::TcpServerOpts;
use anyhow::Result;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};


pub async fn run(options: TcpServerOpts) -> Result<()> {
    let socket = new_tcp_socket(
        options.server_common_opts.interface,
        options.server_common_opts.server_ip,
        options.server_common_opts.port
    )?;

    let listener = TcpListener::from_std(socket.into())?;

    loop {
        let (stream, _client_address) = listener.accept().await?;
    }


    Ok(())
}

async fn handle_stream(mut stream: TcpStream) -> Result<()> {
    let (reader, writer) = stream.split();

    let f_reader = FramedRead::new(reader, LengthDelimitedCodec::new());
    let f_writer = FramedWrite::new(writer, LengthDelimitedCodec::new());

    loop {

    }
    
    todo!()
}