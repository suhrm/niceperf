use clap::{Args, Parser, Subcommand};
use std::net::IpAddr;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Opts {
    #[command(subcommand)]
    mode: Modes,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    dont_fragment: Option<bool>,
}

#[derive(Subcommand, Debug)]
enum Modes {
    /// Set Niceperf throughput to run in server mode
    #[command(arg_required_else_help = true)]
    Server {
        #[command(subcommand)]
        proto: Protocol,
    },

    /// Set Niceperf throughput to run in client mode
    #[command(arg_required_else_help = true)]
    Client {
        #[command(subcommand)]
        proto: Protocol,
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
    /// Set Niceperf ping to run in ICMP mode
    Icmp(ICMPOpts),
}

#[derive(Args, Clone, Debug)]
pub struct CommonOpts {
    /// length of the payload
    #[arg(long, short, default_value = "64")]
    pub len: Option<usize>,
    /// interval between packets in Seconds
    #[arg(long, default_value = "1")]
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
}
#[derive(Args, Clone, Debug)]
pub struct TCPOpts {
    #[command(flatten)]
    pub common_opts: CommonOpts,
    /// Set the source port
    #[arg(long)]
    pub src_port: Option<u16>,
    /// Set the destination port
    #[arg(long)]
    pub dst_port: Option<u16>,
    /// Set the destination address
    #[arg(long)]
    pub dst_addr: Option<IpAddr>,
    /// Congetion control algorithm to use
    #[arg(long, default_value = "reno")]
    pub cc: Option<String>,
    /// Maximum segment size
    #[arg(long, short, default_value = "1460")]
    pub mss: Option<u16>,
}

#[derive(Args, Clone, Debug)]
pub struct UDPOpts {
    #[command(flatten)]
    common_opts: CommonOpts,
    /// Set the source port
    #[arg(long, short)]
    src_port: Option<u16>,
    /// Set the destination port
    #[arg(long, short)]
    dst_port: Option<u16>,
    /// Set the destination address
    #[arg(long, short)]
    dst_addr: Option<IpAddr>,
}
#[derive(Args, Clone, Debug)]
pub struct ICMPOpts {
    #[command(flatten)]
    pub common_opts: CommonOpts,
    /// Set the destination address
    #[arg(long, short)]
    pub dst_addr: IpAddr,
    pub preload: Option<u64>,
}
