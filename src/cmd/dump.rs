use std::path::Path;

use futures::StreamExt;

use crate::{
    archive::Archive,
    cfg::{Cfg, ImapAccount},
    file, hash,
    imap::Session,
    state::State,
};

#[derive(clap::Args, Debug, Clone)]
pub struct Cmd {
    /// Re-download everything from scratch.
    #[clap(short, long)]
    all: bool,
}

impl Cmd {
    pub async fn run(&self, cfg: &Cfg) -> anyhow::Result<()> {
        let archive = Archive::connect(cfg).await?;
        let state = State::connect(&cfg.db_dir).await?;
        for (account_name, account) in &cfg.imap.accounts {
            dump_account(
                account_name,
                account,
                &cfg.obj_dir,
                &state,
                &archive,
                self.all,
            )
            .await?;
        }
        Ok(())
    }
}

#[tracing::instrument(name = "account", skip_all, fields(name = name))]
async fn dump_account(
    name: &str,
    account: &ImapAccount,
    obj_dir: &Path,
    state: &State,
    archive: &Archive,
    all: bool,
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
        let last_seen_uid = state.get(name, &mailbox).await?.unwrap_or(1);
        match session
            .fetch_msgs_from(
                &mailbox,
                all.then_some(1).unwrap_or(last_seen_uid),
            )
            .await
        {
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
                while let Some((uid, raw)) = msgs.next().await {
                    // tracing::info!(uid, "Msg fetched");
                    let hash = hash::sha256(&raw);
                    archive.insert(&hash, &raw[..]).await?;
                    file::write_as_gz(
                        &obj_dir
                            .join(&hash[..2])
                            .join(&hash)
                            .with_extension("eml"),
                        raw,
                    )?;
                    if uid > last_seen_uid {
                        state.set(name, &mailbox, uid).await?;
                    }
                    progress_bar.inc(1);
                    // tracing::info!(uid, "Msg stored");
                }
                progress_bar.finish();
            }
        }
    }
    Ok(())
}
