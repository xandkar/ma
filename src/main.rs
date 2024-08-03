use std::{env, path::PathBuf};

use clap::Parser;
use tracing::{info_span, Instrument};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Working directory (with config and data).
    #[clap(short, long)]
    dir: PathBuf,

    /// Specify log level.
    #[clap(short, long = "log", default_value_t = tracing::Level::INFO)]
    log_level: tracing::Level,

    #[clap(subcommand)]
    command: Cmd,
}

#[derive(Debug, clap::Subcommand)]
enum Cmd {
    /// Download all messages from all mailboxes from all accounts to database.
    Fetch(ma::cmd::fetch::Cmd),

    /// Export fetched messages from database to git-inspired file tree.
    Export(ma::cmd::export::Cmd),

    /// Import exported messages from file tree to database.
    Import(ma::cmd::import::Cmd),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    env::set_current_dir(&cli.dir)?;
    ma::tracing_init(Some(cli.log_level))?;
    tracing::info!(pwd = ?env::current_dir()?, ?cli, "Start");
    let cfg = ma::cfg::Cfg::read_or_init().await?;
    tracing::info!(?cfg, "Config");
    match cli.command {
        Cmd::Fetch(cmd) => {
            cmd.run(&cfg).instrument(info_span!("fetch")).await?;
        }
        Cmd::Export(cmd) => {
            cmd.run(&cfg).instrument(info_span!("export")).await?;
        }
        Cmd::Import(cmd) => {
            cmd.run(&cfg).instrument(info_span!("import")).await?;
        }
    }
    Ok(())
}
