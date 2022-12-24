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
enum Protocol {
    #[command(arg_required_else_help = true)]
    Tcp(CommonOpts),

    Udp(CommonOpts),
    Icmp(CommonOpts),
}

#[derive(Args, Clone, Debug)]
struct CommonOpts {
    /// length of the payload
    #[arg(long, short, default_value = "64")]
    len: Option<usize>,
    /// interval between packets in Seconds
    #[arg(long, default_value = "1")]
    interval: Option<u64>,
    /// number of packets to send
    #[arg(long, short, conflicts_with = "duration")]
    count: Option<u64>,
    #[arg(long, short)]
    /// Interface to bind to
    iface: Option<String>,
    /// Duration of the test in seconds (mutually exclusive with count)
    #[arg(long, short, conflicts_with = "count")]
    duration: Option<u64>,
}
