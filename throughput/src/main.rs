use clap::Parser;

mod args;

fn main() {
    let args = args::Opts::parse();

    println!("{:?}", args)
}
