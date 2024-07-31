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
    #[clap(short, long = "log", default_value_t = tracing::Level::DEBUG)]
    log_level: tracing::Level,

    #[clap(subcommand)]
    command: Cmd,
}

#[derive(Debug, clap::Subcommand)]
enum Cmd {
    /// Download all messages from all mailboxes from all accounts.
    Dump(ma::cmd::dump::Cmd),

    /// Insert dumped messages into database.
    Insert(ma::cmd::insert::Cmd),
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
        Cmd::Dump(cmd) => {
            cmd.run(&cfg).instrument(info_span!("dump")).await?
        }
        Cmd::Insert(cmd) => {
            cmd.run(&cfg).instrument(info_span!("insert")).await?
        }
    }
    Ok(())
}
