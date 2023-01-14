use clap::Parser;
mod args;
mod icmp;
mod logger;
mod tcp;
mod udp;
use anyhow::Result;
use ctor::ctor;
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

// TODO: Move this to a separate test file
#[cfg(test)]
mod test {
    use super::*;
    // These functions are for initializing the python testing environment
    #[ctor]
    fn init_venv() {
        create_venv();
        activate_venv();
        install_deps();
    }
    fn delete_venv() {
        std::process::Command::new("rm")
            .arg("-rf")
            .arg("venv")
            .output()
            .expect("Failed to delete venv");
    }

    fn create_venv() {
        let mut cmd = std::process::Command::new("python3");
        cmd.arg("-m").arg("venv").arg("venv");
        let output = cmd.output().expect("Failed to create venv");
        assert!(output.status.success());
    }
    fn activate_venv() {
        let mut cmd = std::process::Command::new("source");

        cmd.arg("venv/bin/activate");
        let output = cmd.output().expect("Failed to activate venv");
        assert!(output.status.success());
    }
    fn install_deps() {
        let mut cmd = std::process::Command::new("pip");
        cmd.arg("install")
            .arg("-r")
            .arg("integration_test/requirements.txt");
        let output = cmd.output().expect("Failed to install deps");
        assert!(output.status.success());
    }

    #[test]
    fn icmp() {
        init_venv();
        let o = std::process::Command::new("pytest")
            .arg("integration_test/test_icmp.py")
            .output()
            .unwrap();
        assert!(o.status.success());
        delete_venv();
    }
    #[test]
    fn tcp() {
        init_venv();
        let o = std::process::Command::new("pytest")
            .arg("integration_test/test_tcp.py")
            .output()
            .unwrap();
        assert!(o.status.success());
        delete_venv();
    }
    #[test]
    fn udp() {
        init_venv();
        let o = std::process::Command::new("pytest")
            .arg("integration_test/test_udp.py")
            .output()
            .unwrap();
        assert!(o.status.success());
        delete_venv();
    }
}
