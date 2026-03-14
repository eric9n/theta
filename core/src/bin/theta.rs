use anyhow::Result;
use clap::{ArgAction, CommandFactory, Parser, Subcommand};
use std::path::PathBuf;
use theta::cli::{ops, portfolio, signals, snapshot, structure};

#[derive(Parser, Debug)]
#[command(name = "theta")]
#[command(about = "TSLA option monitoring, chain analysis, and portfolio risk")]
#[command(disable_version_flag = true)]
struct ThetaCli {
    #[arg(short = 'V', long = "version", action = ArgAction::SetTrue, global = true)]
    version: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    Snapshot(snapshot::Cli),
    Portfolio(portfolio::Cli),
    Signals(SignalsCommand),
    Structure(StructureCommand),
    Ops(OpsCommand),
}

fn resolved_version() -> String {
    candidate_version_files()
        .into_iter()
        .find_map(|path| {
            std::fs::read_to_string(path).ok().and_then(|contents| {
                let version = contents.trim();
                (!version.is_empty()).then(|| version.to_string())
            })
        })
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

fn candidate_version_files() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(version_file) = std::env::var("THETA_VERSION_FILE") {
        paths.push(PathBuf::from(version_file));
    }

    if let Ok(exe_path) = std::env::current_exe()
        && let Some(bin_dir) = exe_path.parent()
        && let Some(prefix_dir) = bin_dir.parent()
    {
        paths.push(prefix_dir.join("share").join("theta").join("VERSION"));
    }

    paths.push(PathBuf::from("/usr/local/share/theta/VERSION"));
    paths
}

#[derive(clap::Args, Debug)]
#[command(about = "Capture skew snapshots and monitor whether puts or calls are historically rich")]
struct SignalsCommand {
    #[command(subcommand)]
    command: SignalsSubcommand,
}

#[derive(Subcommand, Debug)]
enum SignalsSubcommand {
    Capture(signals::capture::Cli),
    History(signals::history::Cli),
    Monitor(signals::put_call_monitor::Cli),
    IvRank(signals::iv_rank::Cli),
    Extreme(signals::extreme::Cli),
}

#[derive(clap::Args, Debug)]
#[command(about = "Raw option structure diagnostics")]
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
#[command(about = "Operational commands for daemon health and account checks")]
struct OpsCommand {
    #[command(subcommand)]
    command: OpsSubcommand,
}

#[derive(Subcommand, Debug)]
enum OpsSubcommand {
    AccountMonitor(ops::account_monitor::Cli),
    HealthCheck(ops::health_check::Cli),
    #[command(hide = true)]
    StrategyCapture(ops::strategy_capture::Cli),
    #[command(hide = true)]
    StrategyHistory(ops::strategy_history::Cli),
    #[command(hide = true)]
    StrategyMonitor(ops::strategy_monitor::Cli),
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cli = ThetaCli::parse();
    if cli.version {
        println!("{}", resolved_version());
        return Ok(());
    }

    let Some(command) = cli.command else {
        let mut cmd = ThetaCli::command();
        cmd.print_help()?;
        println!();
        return Ok(());
    };

    match command {
        Command::Snapshot(cli) => snapshot::run(cli).await,
        Command::Portfolio(cli) => portfolio::run(cli).await,
        Command::Signals(signals) => match signals.command {
            SignalsSubcommand::Capture(cli) => signals::capture::run(cli).await,
            SignalsSubcommand::History(cli) => signals::history::run(cli),
            SignalsSubcommand::Monitor(cli) => signals::put_call_monitor::run(cli),
            SignalsSubcommand::IvRank(cli) => signals::iv_rank::run(cli),
            SignalsSubcommand::Extreme(cli) => signals::extreme::run(cli),
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
            OpsSubcommand::HealthCheck(cli) => ops::health_check::run(cli).await,
            OpsSubcommand::StrategyCapture(cli) => ops::strategy_capture::run(cli).await,
            OpsSubcommand::StrategyHistory(cli) => ops::strategy_history::run(cli),
            OpsSubcommand::StrategyMonitor(cli) => ops::strategy_monitor::run(cli).await,
        },
    }
}
