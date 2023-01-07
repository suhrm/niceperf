use clap::Parser;
mod args;
mod icmp;
mod logger;
mod tcp;
mod udp;
use anyhow::Result;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = args::Opts::parse();

    match args.mode {
        args::Modes::Client { proto } => match proto {
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
        },
        args::Modes::Server { proto } => match proto {
            args::Protocol::Tcp(opts) => {
                let mut server = tcp::TCPServer::new(opts)?;
                server.run().await?;
            }
            args::Protocol::Udp(opts) => {
                let mut server = udp::UDPServer::new(opts)?;
                server.run().await?;
            }
            args::Protocol::Icmp(opts) => {
                anyhow::bail!("ICMP server not implemented");
            }
        },
    };
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn icmp() {
        let o = std::process::Command::new("pytest")
            .arg("integration_test/test_icmp.py")
            .output()
            .unwrap();
        assert!(o.status.success());
    }
    #[test]
    fn tcp() {
        let o = std::process::Command::new("pytest")
            .arg("integration_test/test_tcp.py")
            .output()
            .unwrap();
        assert!(o.status.success());
    }
    #[test]
    fn udp() {
        let o = std::process::Command::new("pytest")
            .arg("integration_test/test_udp.py")
            .output()
            .unwrap();
        assert!(o.status.success());
    }
}
