use tokio::io;
use tokio::net::TcpListener;
use common::new_tcp_socket;
use crate::args::TcpServerOpts;
use anyhow::Result;


pub async fn run(options: TcpServerOpts) -> Result<()> {
    let socket = new_tcp_socket(
        options.server_common_opts.interface,
        options.server_common_opts.server_ip,
        options.server_common_opts.port
    )?;

    let listener = TcpListener::from_std(socket.into())?;

    loop {
        let (socket, _client_address) = listener.accept().await?;
    }


    Ok(())
}