use clap::Parser;

mod args;
mod tcpserver;
mod messages;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = args::Opts::parse();

    println!("{:?}", args);

    return match args.mode {
        Some(args::Modes::Server { proto }) => {
            match proto {
                args::ServerProtocol::Tcp(options) => {
                    println!("Starting tcp server");
                    tcpserver::run(options).await
                },
                args::ServerProtocol::Udp(..) => {
                    todo!()
                },
                args::ServerProtocol::Quic(..) => {
                    todo!()
                }
            }
        },
        Some(args::Modes::Client { proto: _ }) => {
            todo!()
        },
        None => { Ok({}) }
    };
}
