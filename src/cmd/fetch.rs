use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::task::JoinSet;

use crate::{
    cfg::{Cfg, ImapAccount},
    data,
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
        let db = data::Storage::connect(&cfg.db).await?;
        let db = Arc::new(db);

        // XXX Other than for access to set finish messages, also so that bars
        //     don't disappear from screen when dropped on task error exit.
        let mut bars = HashMap::new();

        let mut account_fetches = JoinSet::new();
        let mp = MultiProgress::new();
        for (account_name, account_cfg) in &cfg.imap.accounts {
            let pb = mp.add(prog_bar_spin(0)?);
            bars.insert(account_name.clone(), pb.clone());
            pb.set_message(format!("{account_name:?}"));
            pb.enable_steady_tick(Duration::from_millis(100));
            account_fetches.spawn({
                let account_name = account_name.to_string();
                let account_cfg = account_cfg.clone();
                let db = Arc::clone(&db);
                let all = self.all;
                async move {
                    let result = fetch_account(
                        &account_name,
                        &account_cfg,
                        &db,
                        all,
                        pb,
                    )
                    .await;
                    (account_name, result)
                }
            });
        }

        let mark_ok = console::style("V").green();
        let mark_err = console::style("X").red();

        while let Some(result) = account_fetches.join_next().await {
            tracing::info!(?result, "Account fetch done.");
            match result {
                Ok((account_name, result)) => {
                    let pb = bars
                        .get(&account_name)
                        .unwrap_or_else(|| unreachable!());
                    pb.set_style(prog_sty_spin_fin()?);
                    match result {
                        Ok(()) => {
                            pb.finish_with_message(format!(
                                "[{mark_ok}] {account_name:?}"
                            ));
                        }
                        Err(_) => {
                            pb.finish_with_message(format!(
                                "[{mark_err}] {account_name:?}"
                            ));
                        }
                    }
                }
                Err(_) => {
                    tracing::error!(
                        ?result,
                        "Failed to join account fetcher."
                    );
                }
            }
        }

        Ok(())
    }
}

#[tracing::instrument(name = "account", skip_all, fields(name = name))]
async fn fetch_account(
    name: &str,
    account: &ImapAccount,
    db: &data::Storage,
    all: bool,
    pb: ProgressBar,
) -> anyhow::Result<()> {
    tracing::info!(?account, "Dump");
    let mut session = Session::new(account).await?;
    let mut mailboxes = session
        .list_mailboxes()
        .await?
        .filter(|mailbox| {
            let ignored = account.ignore_mailboxes.contains(mailbox);
            async move { !ignored }
        })
        .collect::<Vec<String>>()
        .await;
    mailboxes.sort();
    for mailbox in &mailboxes {
        let meta = session.examine(mailbox).await?;
        let exists = meta.exists;
        pb.inc_length(u64::from(exists));
    }
    let m_total = mailboxes.len();
    for (m, mailbox) in mailboxes.into_iter().enumerate() {
        let mailbox_name =
            format!("{name:?} : {mailbox:?} ({} / {})", m, m_total);
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
            Ok((_meta, mut msgs)) => {
                let mut ord_prev: u32 = 0;
                pb.set_message(mailbox_name);
                while let Some((uid, ord_curr, raw)) = msgs.next().await {
                    // TODO Batch insertions.
                    db.store_msg(&raw[..]).await?;
                    if uid > last_seen_uid {
                        db.store_last_seen(name, &mailbox, uid).await?;
                    }
                    pb.inc(u64::from(ord_curr - ord_prev));
                    ord_prev = ord_curr;
                }
            }
        }
    }
    tracing::warn!("Account done.");
    Ok(())
}

fn prog_bar_spin(size: u64) -> anyhow::Result<ProgressBar> {
    let bar = ProgressBar::new(size);
    let sty = prog_sty_spin()?;
    bar.set_style(sty);
    Ok(bar)
}

fn prog_sty_spin() -> anyhow::Result<ProgressStyle> {
    let sty = ProgressStyle::with_template(
        "[{pos:>10} / {len:10} {bar:50.green}] {prefix}[{spinner:.green}] {msg}",
    )?;
    Ok(sty)
}

fn prog_sty_spin_fin() -> anyhow::Result<ProgressStyle> {
    let sty = ProgressStyle::with_template(
        "[{pos:>10} / {len:10} {bar:50.green}] {prefix}{msg}",
    )?;
    Ok(sty)
}
