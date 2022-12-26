use clap::Parser;
mod args;
use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, new_icmp_socket};
use socket2::Socket;
use std::net::IpAddr;

fn main() {
    let args = args::Opts::parse();
}

struct ICMPClient {
    /// ICMP socket
    socket: Socket,
    /// Interface to bind to
    iface: String,
    /// Src IP address of the socket
    src_addr: IpAddr,
    /// Destination IP address
    dst_addr: IpAddr,
    /// Internal counter for sequence number of ICMP packets (Not the same as ip sequence number)
    internal_couter: u128,
    /// How many packets preload before following specified interval.
    preload: u64,
}

impl ICMPClient {
    pub fn new(args: args::ICMPOpts) -> Result<ICMPClient> {
        let iface = args
            .common_opts
            .iface
            .ok_or(anyhow!("No interface specified"))?;
        let iface = iface.as_str();
        let src_addr = interface_to_ipaddr(iface)?;
        let dst_addr = args.dst_addr;
        let socket = new_icmp_socket(Some(iface), None)?;
        Ok(ICMPClient {
            socket,
            iface: iface.to_string(),
            src_addr,
            dst_addr,
            internal_couter: 0,
            // TODO: Check what ping does
            preload: args.preload.unwrap_or(1),
        })
    }
    pub async fn run(&mut self) -> Result<()> {
        Ok(())
    }
}
