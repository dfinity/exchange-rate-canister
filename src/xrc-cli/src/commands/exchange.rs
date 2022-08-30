pub mod list;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[clap(name("canister"))]
pub struct ExchangeCommand {
    #[clap(subcommand)]
    pub subcommand: SubCommand,
}

#[derive(Debug, Subcommand)]
pub enum SubCommand {
    List,
}

pub fn dispatch(cmd: ExchangeCommand) {
    match cmd.subcommand {
        SubCommand::List => list::exec(),
    }
}
