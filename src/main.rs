use std::path::PathBuf;

use clap::Parser;
use futures::StreamExt;
use ma::{cfg::Cfg, imap::Session};

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
    tracing::info!(?cli, "Starting");
    match cli.command {
        Cmd::Dump { dir } => {
            tracing::info!("Fetching");
            let cfg = Cfg::read_or_init(&dir.join("ma.toml")).await?;
            let progress_style = indicatif::ProgressStyle::with_template(
                "{bar:100.green} {pos:>7} / {len:7}",
            )?;
            for (account_name, account) in cfg.imap.accounts {
                tracing::info!(name = ?account_name, data = ?account, "Account");
                let mut session = Session::new(&account).await?;
                let mailboxes = session
                    .list_mailboxes()
                    .await?
                    .collect::<Vec<String>>()
                    .await;
                for mailbox in mailboxes {
                    tracing::info!(name = ?mailbox, "Mailbox");
                    let (meta, mut msgs) =
                        session.fetch_msgs(&mailbox).await?;
                    let progress_bar =
                        indicatif::ProgressBar::new(u64::from(meta.exists));
                    progress_bar.set_style(progress_style.clone());
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
    }
    Ok(())
}
