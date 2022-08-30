use clap::Subcommand;

pub mod exchange;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Used to retrieve exchange information.
    Exchange(exchange::ExchangeCommand),
}

pub fn dispatch(command: Command) {
    match command {
        Command::Exchange(cmd) => exchange::dispatch(cmd),
    }
}
