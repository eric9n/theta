use anyhow::Result;
use clap::{Parser, Subcommand};
use theta::cli::{ops, portfolio, signals, snapshot, structure};

#[derive(Parser, Debug)]
#[command(name = "theta")]
#[command(about = "Unified CLI for market snapshots, structure signals, and portfolio tracking")]
struct ThetaCli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Snapshot(snapshot::Cli),
    Portfolio(portfolio::Cli),
    Signals(SignalsCommand),
    Structure(StructureCommand),
    Ops(OpsCommand),
}

#[derive(clap::Args, Debug)]
#[command(about = "Snapshot capture, history, and relative/extreme signal analysis")]
struct SignalsCommand {
    #[command(subcommand)]
    command: SignalsSubcommand,
}

#[derive(Subcommand, Debug)]
enum SignalsSubcommand {
    Capture(signals::capture::Cli),
    History(signals::history::Cli),
    IvRank(signals::iv_rank::Cli),
    Extreme(signals::extreme::Cli),
    RelativeExtreme(signals::relative_extreme::Cli),
}

#[derive(clap::Args, Debug)]
#[command(about = "Single-expiry and term-structure options structure analysis")]
struct StructureCommand {
    #[command(subcommand)]
    command: StructureSubcommand,
}

#[derive(Subcommand, Debug)]
enum StructureSubcommand {
    Skew(structure::skew::Cli),
    Smile(structure::smile::Cli),
    PutCallBias(structure::put_call_bias::Cli),
    MarketTone(structure::market_tone::Cli),
    TermStructure(structure::term_structure::Cli),
}

#[derive(clap::Args, Debug)]
#[command(about = "Operational commands for recurring account monitoring workflows")]
struct OpsCommand {
    #[command(subcommand)]
    command: OpsSubcommand,
}

#[derive(Subcommand, Debug)]
enum OpsSubcommand {
    AccountMonitor(ops::account_monitor::Cli),
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cli = ThetaCli::parse();
    match cli.command {
        Command::Snapshot(cli) => snapshot::run(cli).await,
        Command::Portfolio(cli) => portfolio::run(cli).await,
        Command::Signals(signals) => match signals.command {
            SignalsSubcommand::Capture(cli) => signals::capture::run(cli).await,
            SignalsSubcommand::History(cli) => signals::history::run(cli),
            SignalsSubcommand::IvRank(cli) => signals::iv_rank::run(cli),
            SignalsSubcommand::Extreme(cli) => signals::extreme::run(cli),
            SignalsSubcommand::RelativeExtreme(cli) => signals::relative_extreme::run(cli),
        },
        Command::Structure(structure) => match structure.command {
            StructureSubcommand::Skew(cli) => structure::skew::run(cli).await,
            StructureSubcommand::Smile(cli) => structure::smile::run(cli).await,
            StructureSubcommand::PutCallBias(cli) => structure::put_call_bias::run(cli).await,
            StructureSubcommand::MarketTone(cli) => structure::market_tone::run(cli).await,
            StructureSubcommand::TermStructure(cli) => structure::term_structure::run(cli).await,
        },
        Command::Ops(ops) => match ops.command {
            OpsSubcommand::AccountMonitor(cli) => ops::account_monitor::run(cli).await,
        },
    }
}
