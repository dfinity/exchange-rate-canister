use clap::{Parser, Subcommand};
use xrc_tests::scenarios;

#[derive(Subcommand)]
enum Scenarios {
    Basic,
    Cache,
}

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    scenario: Scenarios,
}

fn main() {
    let cli = Cli::parse();

    match cli.scenario {
        Scenarios::Basic => scenarios::basic(),
        Scenarios::Cache => scenarios::caching(),
    };
}
