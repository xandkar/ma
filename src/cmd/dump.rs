use std::path::Path;

use futures::StreamExt;

use crate::{
    cfg::{Cfg, ImapAccount},
    file, hash,
    imap::Session,
};

#[derive(clap::Args, Debug, Clone)]
pub struct Cmd {}

impl Cmd {
    pub async fn run(&self, cfg: &Cfg) -> anyhow::Result<()> {
        for (account_name, account) in &cfg.imap.accounts {
            dump_account(account_name, account, &cfg.obj_dir).await?;
        }
        Ok(())
    }
}

#[tracing::instrument(name = "account", skip_all, fields(name = name))]
async fn dump_account(
    name: &str,
    account: &ImapAccount,
    obj_dir: &Path,
) -> anyhow::Result<()> {
    tracing::info!(?account, "Dump");
    let mut session = Session::new(account).await?;
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
                    let digest = hash::sha256(&raw);
                    file::write_as_gz(
                        &obj_dir
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
