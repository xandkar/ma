use std::{env, path::PathBuf};

use clap::Parser;
use tracing::{info_span, Instrument};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Working directory (with config and data).
    #[clap(short, long, default_value = ".")]
    dir: PathBuf,

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

    /// Experimental analyses.
    Analyze(ma::cmd::analyze::Cmd),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    human_panic_setup();
    let cli = Cli::parse();
    env::set_current_dir(&cli.dir)?;
    let _logger_guard = ma::tracing::init().await?;
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
        Cmd::Analyze(cmd) => {
            cmd.run(&cfg).instrument(info_span!("analyze")).await?;
        }
    }
    Ok(())
}

fn human_panic_setup() {
    macro_rules! repo {
        () => {
            env!("CARGO_PKG_REPOSITORY")
        };
    }
    human_panic::setup_panic!(human_panic::Metadata::new(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    )
    .authors(env!("CARGO_PKG_AUTHORS"))
    .homepage(repo!())
    .support(concat!("- Submit an issue at ", repo!(), "/issues")));
}
