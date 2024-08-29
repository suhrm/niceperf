use anyhow::Result;
use clap::Parser;
use common::{QuicClient, QuicServer, TCPSocket};
mod args;
fn main() -> Result<()> {
    let opts: args::Opts = args::Opts::parse();
    match opts.mode {
        args::Modes::Server { proto } => {
            match proto {
                args::Protocol::Tcp(_) => todo!(),
                args::Protocol::Udp(_) => todo!(),
            }
        }
        args::Modes::Client { proto } => {
            match proto {
                args::Protocol::Tcp(opts) => {
                    dbg!(opts);
                }
                args::Protocol::Udp(opts) => {
                    dbg!(opts);
                }
            }
        }
    }

    Ok(())
}
// struct Client<T: Protocol> {
//     ctrl_socket: TCPSocket,
//     protocol: T,
// }
