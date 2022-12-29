use clap::Parser;
mod args;
mod icmp;
mod logger;
use anyhow::Result;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = args::Opts::parse();

    match args.mode {
        args::Modes::Client { proto } => match proto {
            args::Protocol::Tcp(opts) => {
                println!("TCP");
            }
            args::Protocol::Udp(opts) => {
                println!("UDP");
            }
            args::Protocol::Icmp(opts) => {
                let client = icmp::ICMPClient::new(opts);
                client?.run().await?;
            }
        },
        args::Modes::Server { proto } => match proto {
            args::Protocol::Tcp(opts) => {
                println!("TCP");
            }
            args::Protocol::Udp(opts) => {
                println!("UDP");
            }
            args::Protocol::Icmp(opts) => {
                println!("ICMP");
            }
        },
    };
    Ok(())
    // match args.mode {}
}
