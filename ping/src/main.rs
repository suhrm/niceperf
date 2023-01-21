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

// TODO: Move this to a separate test file
#[cfg(test)]
mod test {
    use super::*;
    // These functions are for initializing the python testing environment
    fn init_venv() {
        create_venv();
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
    fn install_deps() {
        let mut cmd = std::process::Command::new(format!("venv/bin/pip"));
        cmd.arg("install")
            .arg("-r")
            .arg(format!("integration_test/requirements.in"));
        let output = cmd.output().expect("Failed to install deps");
        assert!(output.status.success());
    }

    #[test]
    fn integration_test() {
        init_venv();
        let mut cmd = std::process::Command::new(format!("venv/bin/python"));
        cmd.arg("-m").arg("pytest").arg("integration_test/");
        let output = cmd.output().expect("Failed to run test");
        // For printing the output of the of the pytest tests
        println!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.status.success());
        delete_venv();
    }
}
