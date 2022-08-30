mod commands;

use clap::Parser;

#[derive(Debug, Parser)]

struct Cli {
    #[clap(subcommand)]
    command: commands::Command,
}

fn main() {
    let cli = Cli::parse();
    println!("{:#?}", cli);
    commands::dispatch(cli.command);
}
