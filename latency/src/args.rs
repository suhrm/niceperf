use std::net::IpAddr;

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Opts {
    #[command(subcommand)]
    pub mode: Modes,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    dont_fragment: Option<bool>,
}

#[derive(Subcommand, Debug)]
pub enum Modes {
    /// Set Niceperf throughput to run in server mode
    #[command(arg_required_else_help = true)]
    Server {
        #[command(subcommand)]
        proto: Protocol,
        /// Set the listen address for the control channel
        #[arg(long, default_value = "0.0.0.0")]
        listen_addr: IpAddr,
        /// Set the listen port for the control channel
        #[arg(long, default_value = "4443")]
        listen_port: u16,
    },

    /// Set Niceperf throughput to run in client mode
    #[command(arg_required_else_help = true)]
    Client {
        #[command(subcommand)]
        proto: Protocol,
        #[arg(long)]
        /// Set the destination address for the control channel
        dst_addr: IpAddr,
        /// Set the destination port for the control channel
        #[arg(long, default_value = "4443")]
        dst_port: u16,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum Protocol {
    #[command(arg_required_else_help = true)]
    /// Set Niceperf ping to run in TCP mode
    Tcp(TCPOpts),
    #[command(arg_required_else_help = true)]
    /// Set Niceperf ping to run in UDP mode
    Udp(UDPOpts),
    #[command(arg_required_else_help = true)]
    /// Set Niceperf ping to run in QUIC mode
    Quic(QuicOpts),
}

#[derive(Args, Clone, Debug)]
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
#[derive(Args, Clone, Debug)]
pub struct TCPOpts {
    #[command(flatten)]
    pub common_opts: CommonOpts,
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
#[derive(Args, Clone, Debug)]
pub struct UDPOpts {
    #[command(flatten)]
    pub common_opts: CommonOpts,
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

#[derive(Args, Clone, Debug)]
pub struct QuicOpts {
    #[command(flatten)]
    pub common_opts: CommonOpts,
    /// Set the destination port
    #[arg(long)]
    pub dst_port: u16,
    /// Set the destination address
    #[arg(long)]
    pub dst_addr: IpAddr,
}
