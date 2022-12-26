use clap::Parser;
mod args;
use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, ICMPSocket};
use core::mem::MaybeUninit;
use socket2::Socket;
use std::net::IpAddr;
use tokio::io::unix::AsyncFd;

fn main() {
    let args = args::Opts::parse();
}

struct ICMPClient {
    /// ICMP socket
    socket: ICMPSocket,
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
        let socket = ICMPSocket::new(Some(iface), None)?;
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

struct AsyncICMP {
    inner: AsyncFd<ICMPSocket>,
}

impl AsyncICMP {
    pub fn new(socket: ICMPSocket) -> Result<AsyncICMP> {
        Ok(AsyncICMP {
            inner: AsyncFd::new(socket)?,
        })
    }

    /* pub async fn send(&mut self, packet: &[u8]) -> Result<()> {
        let mut guard = self.inner.writable().await;
        guard.try_io(|inner| inner.get_mut().send(packet))?;
        Ok(())
    } */

    pub async fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            let mut guard = self.inner.readable().await?;
            let uninit_slice = unsafe { core::mem::transmute(&mut *buf) };

            match guard
                .try_io(|inner| inner.get_ref().get_ref().recv(uninit_slice))
            {
                Ok(Ok(n)) => return Ok(n),
                Ok(Err(e)) => return Err(e.into()),
                Err(_would_block) => continue,
            }
        }
    }
}
