use anyhow::Result;
use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{
    generate,
    shells::{Bash, Elvish, Fish, PowerShell, Zsh},
};
use std::io;
use std::path::PathBuf;
use theta::cli::{ops, portfolio, signals, snapshot, structure};

#[derive(Parser, Debug)]
#[command(name = "theta")]
#[command(about = "Unified CLI for market snapshots, structure signals, and portfolio tracking")]
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
    Completion(CompletionCommand),
}

#[derive(clap::Args, Debug)]
#[command(about = "Generate shell completion scripts")]
struct CompletionCommand {
    #[arg(long, value_enum)]
    shell: CompletionShell,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh,
}

fn emit_completion(shell: CompletionShell) {
    let mut cmd = ThetaCli::command();
    match shell {
        CompletionShell::Bash => generate(Bash, &mut cmd, "theta", &mut io::stdout()),
        CompletionShell::Elvish => generate(Elvish, &mut cmd, "theta", &mut io::stdout()),
        CompletionShell::Fish => generate(Fish, &mut cmd, "theta", &mut io::stdout()),
        CompletionShell::Powershell => generate(PowerShell, &mut cmd, "theta", &mut io::stdout()),
        CompletionShell::Zsh => generate(Zsh, &mut cmd, "theta", &mut io::stdout()),
    }
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
    HealthCheck(ops::health_check::Cli),
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
            OpsSubcommand::HealthCheck(cli) => ops::health_check::run(cli).await,
        },
        Command::Completion(completion) => {
            emit_completion(completion.shell);
            Ok(())
        }
    }
}
