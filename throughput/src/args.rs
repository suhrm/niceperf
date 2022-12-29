use std::net::IpAddr;

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Opts {
    #[command(subcommand)]
    pub mode: Option<Modes>,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub dont_fragment: Option<bool>,

    /// Provide a config file instead of the options above
    #[arg(long)]
    pub config_file: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Modes {
    /// Set Niceperf throughput to run in server mode
    #[command(arg_required_else_help = true)]
    Server {
        #[command(subcommand)]
        proto: ServerProtocol,
    },

    /// Set Niceperf throughput to run in client mode
    #[command(arg_required_else_help = true)]
    Client {
        #[command(subcommand)]
        proto: ClientProtocol,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ServerProtocol {
    #[command(arg_required_else_help = true)]
    Tcp(TcpServerOpts),

    #[command(arg_required_else_help = true)]
    Udp(UdpServerOpts),

    #[command(arg_required_else_help = true)]
    Quic(QuicServerOpts),
}

#[derive(Args, Clone, Debug)]
pub struct TcpServerOpts {
    #[command(flatten)]
    pub server_common_opts: ServerCommonOpts,
}

#[derive(Args, Clone, Debug)]
pub struct UdpServerOpts {
    #[command(flatten)]
    server_common_opts: ServerCommonOpts,
}

#[derive(Args, Clone, Debug)]
pub struct QuicServerOpts {
    #[command(flatten)]
    server_common_opts: ServerCommonOpts,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ClientProtocol {
    #[command(arg_required_else_help = true)]
    Tcp(TcpClientOpts),

    #[command(arg_required_else_help = true)]
    Udp(UdpClientOpts),

    #[command(arg_required_else_help = true)]
    Quic(QuicClientOpts),
}

#[derive(Args, Clone, Debug)]
pub struct TcpClientOpts {
    #[command(flatten)]
    client_common_opts: ClientCommonOpts,
}

#[derive(Args, Clone, Debug)]
pub struct UdpClientOpts {
    #[command(flatten)]
    client_common_opts: ClientCommonOpts,
}

#[derive(Args, Clone, Debug)]
pub struct QuicClientOpts {
    #[command(flatten)]
    client_common_opts: ClientCommonOpts,
}

#[derive(Args, Clone, Debug)]
pub struct ServerCommonOpts {
    /// IP the server will listen on
    #[arg(short, long)]
    pub server_ip: IpAddr,

    /// Port the server will listen on
    #[arg(short, long)]
    pub port: u16,

    /// Interface to bind to
    #[arg(short, long)]
    pub interface: Option<String>,
}

#[derive(Args, Clone, Debug)]
pub struct ClientCommonOpts {
    /// IP of the server
    #[arg(short = 'c', long = "client")]
    server_ip: Option<IpAddr>,

    /// Port of the server
    #[arg(short, long)]
    port: Option<u16>,

    /// Interface to bind to
    #[arg(short, long)]
    interface: Option<String>,
}
