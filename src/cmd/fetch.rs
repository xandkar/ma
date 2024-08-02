use futures::StreamExt;

use crate::{
    cfg::{Cfg, ImapAccount},
    data::DataBase,
    imap::Session,
};

#[derive(clap::Args, Debug, Clone)]
pub struct Cmd {
    /// Re-download everything from scratch.
    #[clap(short, long)]
    all: bool,
}

impl Cmd {
    pub async fn run(&self, cfg: &Cfg) -> anyhow::Result<()> {
        let db = DataBase::connect(&cfg.db).await?;
        for (account_name, account) in &cfg.imap.accounts {
            if let Err(error) =
                fetch_account(account_name, account, &db, self.all).await
            {
                tracing::error!(
                    name = ?account_name,
                    ?error,
                    "Failed to fetch account."
                );
            }
        }
        Ok(())
    }
}

#[tracing::instrument(name = "account", skip_all, fields(name = name))]
async fn fetch_account(
    name: &str,
    account: &ImapAccount,
    db: &DataBase,
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
        let last_seen_uid =
            db.fetch_last_seen(name, &mailbox).await?.unwrap_or(1);
        let first_uid = if all { 1 } else { last_seen_uid };
        match session.fetch_msgs_from(&mailbox, first_uid).await {
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
                    db.store_msg(&raw[..]).await?;
                    if uid > last_seen_uid {
                        db.store_last_seen(name, &mailbox, uid).await?;
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
