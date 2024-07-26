use std::path::{Path, PathBuf};

use clap::Parser;
use futures::StreamExt;
use ma::{
    cfg::{Cfg, ImapAccount},
    imap::Session,
};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Specify log level.
    #[clap(short, long = "log", default_value_t = tracing::Level::DEBUG)]
    log_level: tracing::Level,

    #[clap(subcommand)]
    command: Cmd,
}

#[derive(Debug, clap::Subcommand)]
enum Cmd {
    Dump { dir: PathBuf },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    ma::tracing_init(Some(cli.log_level))?;
    tracing::info!(?cli, "Start");
    match cli.command {
        Cmd::Dump { dir } => {
            let cfg = Cfg::read_or_init(&dir.join("ma.toml")).await?;
            tracing::info!(?cfg, "Config");
            for (account_name, account) in cfg.imap.accounts {
                dump(&account_name, account, &dir).await?;
            }
        }
    }
    Ok(())
}

#[tracing::instrument(skip_all, fields(account = name))]
async fn dump(
    name: &str,
    account: ImapAccount,
    dir: &Path,
) -> anyhow::Result<()> {
    tracing::info!(?account, "Dump");
    let mut session = Session::new(&account).await?;
    let mut mailboxes = session
        .list_mailboxes()
        .await?
        .collect::<Vec<String>>()
        .await;
    mailboxes.sort();
    for mailbox in mailboxes {
        // tracing::info!(name = ?mailbox, "Mailbox");
        eprintln!("{name:?} / {mailbox:?}:");
        match session.fetch_msgs(&mailbox).await {
            Err(error) => {
                tracing::error!(
                    ?mailbox,
                    ?error,
                    "Failed to fetch mailbox. Skipping it."
                );
            }
            Ok((meta, mut msgs)) => {
                let progress_bar =
                    indicatif::ProgressBar::new(u64::from(meta.exists));
                let progress_style = indicatif::ProgressStyle::with_template(
                    "{bar:100.green} {pos:>7} / {len:7}",
                )?;
                progress_bar.set_style(progress_style);
                progress_bar.tick();
                while let Some((_uid, raw)) = msgs.next().await {
                    // tracing::info!(uid, "Msg fetched");
                    let digest = ma::hash::sha256(&raw);
                    ma::file::write_as_gz(
                        &dir.join("dump")
                            .join(&digest[..2])
                            .join(&digest)
                            .with_extension("eml"),
                        raw,
                    )?;
                    progress_bar.inc(1);
                    // tracing::info!(uid, "Msg stored");
                }
                progress_bar.finish();
            }
        }
    }
    Ok(())
}
