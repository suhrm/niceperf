use clap::Parser;
mod args;
mod icmp;
mod logger;
mod non_async;
mod tcp;
mod udp;
use anyhow::Result;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // console_subscriber::init();
    let args = args::Opts::parse();
    match args.mode {
        args::Modes::Client { proto } => {
            match proto {
                args::Protocol::Tcp(opts) => {
                    let mut client = tcp::TCPClient::new(opts)?;
                    client.run().await?;
                }
                args::Protocol::Udp(opts) => {
                    let mut client = udp::UDPClient::new(opts)?;
                    client.run().await?;
                }
                args::Protocol::Icmp(opts) => {
                    let mut client = icmp::ICMPClient::new(opts)?;
                    client.run().await?;
                }
            }
        }
        args::Modes::Server { proto } => {
            match proto {
                args::Protocol::Tcp(opts) => {
                    let mut server = tcp::TCPServer::new(opts)?;
                    server.run().await?;
                }
                args::Protocol::Udp(opts) => {
                    let mut server = udp::UDPServer::new(opts)?;
                    server.run().await?;
                }
                args::Protocol::Icmp(..) => {
                    anyhow::bail!("ICMP server not implemented");
                }
            }
        }
    };

    Ok(())
}

#[cfg(test)]
mod test {
    use common::{delete_venv, init_venv, run_pytest};

    // These functions are for running pytests based on the dummynet network
    // namespace testing environment
    #[test]
    fn integration_test() {
        init_venv();
        run_pytest();
        delete_venv();
    }
}
