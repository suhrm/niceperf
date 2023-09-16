use std::net::IpAddr;

use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug, Deserialize, Serialize)]
#[command(author, version, about)]
pub struct Opts {
    #[command(subcommand)]
    pub mode: Modes,
    // #[arg(long, action = clap::ArgAction::SetTrue)]
    // dont_fragment: Option<bool>,
}

#[derive(Subcommand, Debug, Deserialize, Serialize)]
pub enum Modes {
    /// Set Niceperf throughput to run in server mode
    #[command(arg_required_else_help = true)]
    Server(Server),

    /// Set Niceperf throughput to run in client mode
    #[command(arg_required_else_help = true)]
    Client(Client),
}
#[derive(Args, Debug, Deserialize, Serialize)]
pub struct Client {
    #[command(subcommand)]
    pub config_type: ConfigType,
    /// Set Niceperf latency to run from config file

    /// Set the destination address for the control channel
    #[arg(long)]
    pub dst_addr: IpAddr,
    /// Set the destination port for the control channel
    #[arg(long, default_value = "4443")]
    pub dst_port: u16,
    /// Set the filepath to the cert file
    #[arg(long)]
    pub cert_path: Option<String>,
}

#[derive(Subcommand, Debug, Deserialize, Serialize)]
pub enum ConfigType {
    #[command(arg_required_else_help = true)]
    /// Set Niceperf latency to run from config file
    File {
        #[arg(long, short)]
        path: String,
    },

    Protocol {
        #[command(subcommand)]
        proto: ProtocolType,
        #[clap(flatten)]
        common_opts: CommonOpts,
    },
}

#[derive(Args, Debug, Deserialize, Serialize, Clone)]
pub struct Server {
    /// Set the listen address for the control channel
    #[arg(long, default_value = "0.0.0.0")]
    pub listen_addr: IpAddr,
    /// Set the listen port for the control channel
    #[arg(long, default_value = "4443")]
    pub listen_port: u16,
    /// Port range to use for data channel, if not specified, will use
    /// ephemeral port
    #[arg(long, value_parser, num_args = 1.., value_delimiter = ',')]
    pub data_port_range: Option<Vec<u16>>,
}

#[derive(Subcommand, Debug, Clone, Deserialize, Serialize)]
pub enum ProtocolType {
    #[command(arg_required_else_help = true)]
    /// Set Niceperf latency to run in TCP mode
    Tcp(TCPOpts),
    #[command(arg_required_else_help = true)]
    /// Set Niceperf latency to run in UDP mode
    Udp(UDPOpts),
    #[command(arg_required_else_help = true)]
    /// Set Niceperf latency to run in QUIC mode
    Quic(QuicOpts),
}

#[derive(Args, Clone, Debug, Deserialize, Serialize)]
pub struct FileOpts {
    /// Path to the config file
    #[arg(long, short)]
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub common_opts: CommonOpts,
    pub proto: ProtocolType,
}

#[derive(Args, Clone, Debug, Deserialize, Serialize, Default)]
pub struct CommonOpts {
    /// length of the payload
    #[arg(long, short, default_value = "64")]
    pub len: Option<usize>,
    /// interval between packets in miliseconds
    #[arg(long, default_value = "1000")]
    pub interval: Option<u64>,
    /// number of packets to send
    #[arg(long, short, conflicts_with = "duration")]
    pub count: Option<u64>,
    #[arg(long, short)]
    /// Interface to bind to
    pub iface: Option<String>,
    /// Duration of the test in seconds (mutually exclusive with count)
    #[arg(long, short, conflicts_with = "count")]
    pub duration: Option<u64>,
    /// Save the result to a file
    #[arg(long)]
    pub file: Option<String>,
    /// Set the amount of packets to preload before following the interval
    #[arg(long, default_value = "1")]
    pub preload: Option<u64>,
}
#[derive(Args, Clone, Debug, Deserialize, Serialize)]
pub struct TCPOpts {
    // TODO: Same as with UDP, do we need the source port and address?
    /// Set the source port
    // #[arg(long)]
    // pub src_port: Option<u16>,
    /// Set the destination port
    #[arg(long)]
    pub dst_port: u16,
    /// Set the destination address
    #[arg(long)]
    pub dst_addr: IpAddr,
    /// Congetion control algorithm to use
    #[arg(long, default_value = "reno")]
    pub cc: Option<String>,
    /// Maximum segment size
    #[arg(long, short, default_value = "1460")]
    pub mss: Option<u16>,
}
// TODO:: Handle server and client options for UDP separately?
#[derive(Args, Clone, Debug, Deserialize, Serialize)]
pub struct UDPOpts {
    // TODO: is Src Port needed?
    /* /// Set the source port
    #[arg(long, short)]
    pub src_port: Option<u16>, */
    /// Set the destination port
    #[arg(long)]
    pub dst_port: u16,
    /// Set the destination address
    #[arg(long)]
    pub dst_addr: IpAddr,
}

#[derive(Args, Clone, Debug, Deserialize, Serialize)]
pub struct QuicOpts {
    /// Set the destination port
    #[arg(long)]
    pub dst_port: u16,
    /// Set the destination address
    #[arg(long)]
    pub dst_addr: IpAddr,
}
