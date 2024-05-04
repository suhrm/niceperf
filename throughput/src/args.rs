use std::net::IpAddr;

use anyhow::{anyhow, Result};
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
    /// Set Niceperf ping to run in server mode
    #[command(arg_required_else_help = true)]
    Server {
        #[command(subcommand)]
        proto: Protocol,
    },

    /// Set Niceperf ping to run in client mode
    #[command(arg_required_else_help = true)]
    Client {
        #[command(subcommand)]
        proto: Protocol,
    },
}
#[derive(Clone, Debug)]
struct Bandwidth(u64);

impl Into<u64> for Bandwidth {
    fn into(self) -> u64 {
       self.0
    }
}

impl std::str::FromStr for Bandwidth {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut num = s
            .chars()
            .take_while(|c| c.is_digit(10))
            .collect::<String>()
            .parse::<u64>()?;
        let unit = s
            .chars()
            .last()
            .ok_or(anyhow::anyhow!("Error parsing SI unit"))?
            .to_ascii_lowercase();
        match unit {
            'k' => num *= 1000,
            'm' => num *= 1000000,
            'g' => num *= 1000000000,
            _ => (),
        }
        Ok(Bandwidth(num))
    }
}

#[derive(Args, Clone, Debug)]
pub struct CommonOpts {
    /// length of the payload
    #[arg(long, short, default_value = "64")]
    pub len: Option<usize>,
    /// bandwidth target e.g. 1k, 1m, 1g
    #[arg(long, default_value = "1m", value_parser = clap::value_parser!(Bandwidth))]
    pub bandwidth: Option<Bandwidth>,
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
    /// Set whether to log or not
    #[arg(long, default_value = "true", conflicts_with = "file")]
    pub log: Option<bool>,
    /// Set the destination address
    #[arg(long)]
    pub dst_addr: IpAddr,
    /// Set the source address
    #[arg(long, conflicts_with = "iface")]
    pub src_addr: Option<IpAddr>,
}
#[derive(Args, Clone, Debug)]
pub struct TCPOpts {
    #[command(flatten)]
    pub common_opts: CommonOpts,
    /// Set the destination port
    #[arg(long)]
    pub dst_port: u16,
    /// Set the source port
    #[arg(long)]
    pub src_port: Option<u16>,
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
    /// Set the destination port
    #[arg(long)]
    pub dst_port: u16,
    /// Set the source port
    #[arg(long)]
    pub src_port: Option<u16>,
}
#[derive(Subcommand, Debug, Clone)]
pub enum Protocol {
    #[command(arg_required_else_help = true)]
    /// Set Niceperf ping to run in TCP mode
    Tcp(TCPOpts),
    #[command(arg_required_else_help = true)]
    /// Set Niceperf ping to run in UDP mode
    Udp(UDPOpts),
}
