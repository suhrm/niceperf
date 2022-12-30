use std::{
    net::IpAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, UDPSocket};
use tokio::net::UdpSocket as tokioUdpSocket;

use crate::{
    args,
    logger::{PingLogger, PingResult},
};

pub struct UDPClient {
    socket: tokio::net::UdpSocket,
    common: args::CommonOpts,
    src_addr: IpAddr,
    dst_addr: IpAddr,
    internal_couter: u128,
    identifier: u16,
    src_port: Option<u16>,
    dst_port: u16,
    logger: Option<PingLogger>,
}

impl UDPClient {
    pub fn new(args: args::UDPOpts) -> Result<UDPClient> {
        let iface = args
            .common_opts
            .iface
            .clone()
            .ok_or(anyhow!("No interface specified"))?;
        let iface = iface.as_str();
        let src_addr = interface_to_ipaddr(iface)?;
        let dst_addr = args.dst_addr;
        let dst_port = args.dst_port;

        let socket = UDPSocket::new(Some(iface), None)?;
        let socket = tokioUdpSocket::from_std(
            socket.get_ref().try_clone()?.try_into()?,
        )?;

        let logger = match args.common_opts.file.clone() {
            Some(file_name) => Some(PingLogger::new(file_name)?),
            None => None,
        };

        Ok(UDPClient {
            socket,
            common: args.common_opts,
            src_addr,
            dst_addr,
            internal_couter: 0,
            identifier: rand::random::<u16>(),
            src_port: None,
            dst_port,
            logger,
        })
    }
}
