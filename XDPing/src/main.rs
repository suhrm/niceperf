use std::{
    ffi::{CStr, CString},
    sync::Arc,
};

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
struct Opts {
    #[arg(short, long, default_value = "lo")]
    ifname: String,
    #[arg(short, long, default_value = "0")]
    queue: u32,
}

fn main() -> Result<()> {
    let opts = Opts::parse();

    let ifname = CString::new(opts.ifname.as_str())?;
    let mut interface = xdpilone::IfInfo::invalid();
    interface
        .from_name(&ifname)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    interface.set_queue(opts.queue);


    let mut socket = xdpilone::Socket::new(&interface);

    Ok(())
}
