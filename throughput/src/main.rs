use clap::{Parser, Subcommand};
use std::net::IpAddr;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[command(subcommand)]
    mode: Modes,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    dont_fragment: Option<bool>,
}

#[derive(Subcommand, Debug)]
enum Modes {
    /// Set Niceperf throughput to run in server mode
    #[command(arg_required_else_help = true)]
    Server {
        /// IP the server will listen on
        #[arg(short, long, conflicts_with = "config_file")]
        server_ip: Option<IpAddr>,

        /// Port the server will listen on
        #[arg(short, long, conflicts_with = "config_file")]
        port: Option<u16>,

        /// Interface to bind to
        #[arg(short, long, conflicts_with = "config_file")]
        interface: Option<String>,

        /// Provide a config file instead of the options above
        #[arg(long)]
        config_file: Option<String>,
    },

    /// Set Niceperf throughput to run in client mode
    #[command(arg_required_else_help = true)]
    Client {
        /// IP the client will listen on
        #[arg(short, long)]
        client_ip: Option<IpAddr>,

        /// Port the client will listen on
        #[arg(short, long)]
        port: Option<u16>,

        /// Interface to bind to
        #[arg(short, long)]
        interface: Option<String>,

        /// Provide a config file instead of the options above
        #[arg(long)]
        config_file: Option<String>,
    },
}

fn main() {
    let args = Args::parse();

    println!("{:?}", args)
}
