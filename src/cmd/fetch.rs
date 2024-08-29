use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::task::{self, JoinSet};

use crate::{
    cfg::{Cfg, ImapAccount},
    data,
    imap::{self, Session},
};

const MAX_ERR_MSG_LEN: usize = 50;

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
        let mut task_bar: HashMap<task::Id, ProgressBar> = HashMap::new();
        let mut task_account: HashMap<task::Id, String> = HashMap::new();
        let mut tasks = JoinSet::new();
        let mp = MultiProgress::new();
        for (account_name, account_cfg) in &cfg.imap.accounts {
            let pb_inside_task: ProgressBar = mp.add(prog_bar_spin(0)?);
            pb_inside_task.set_message(format!("{account_name:?}"));
            pb_inside_task.enable_steady_tick(Duration::from_millis(100));
            let pb_outside_task = pb_inside_task.clone();
            let handle = tasks.spawn({
                let account_name = account_name.to_string();
                let account_cfg = account_cfg.clone();
                let db = Arc::clone(&db);
                let all = self.all;
                async move {
                    fetch_account(
                        task::id(),
                        &account_name,
                        &account_cfg,
                        &db,
                        all,
                        pb_inside_task,
                    )
                    .await
                }
            });
            let task_id = handle.id();
            task_account.insert(task_id, account_name.to_string());
            task_bar.insert(task_id, pb_outside_task);
        }

        while let Some(result) = tasks.join_next_with_id().await {
            match result {
                Ok((task_id, result)) => {
                    let account_name = task_account
                        .get(&task_id)
                        .unwrap_or_else(|| unreachable!());
                    let pb = task_bar
                        .get(&task_id)
                        .unwrap_or_else(|| unreachable!());
                    pb.set_style(prog_sty_spin_fin()?);
                    match result {
                        Ok(()) => {
                            tracing::info!(
                                ?account_name,
                                ?task_id,
                                "Account fetch succeeded."
                            );
                            prog_fin_ok(account_name, &pb);
                        }
                        Err(error) => {
                            tracing::error!(
                                ?account_name,
                                ?task_id,
                                ?error,
                                "Account fetch failed."
                            );
                            prog_fin_err(
                                account_name,
                                &pb,
                                &error.root_cause().to_string(),
                            );
                        }
                    }
                }
                Err(e) if e.is_cancelled() => {
                    let error: task::JoinError = e;
                    let task_id = error.id();
                    let account_name = task_account
                        .get(&task_id)
                        .unwrap_or_else(|| unreachable!());
                    let pb = task_bar
                        .get(&task_id)
                        .unwrap_or_else(|| unreachable!());
                    tracing::error!(
                        ?account_name,
                        ?task_id,
                        ?error,
                        "Account fetch cancelled."
                    );
                    prog_fin_err(account_name, &pb, &error.to_string());
                }
                Err(e) if e.is_panic() => {
                    let error: task::JoinError = e;
                    let task_id = error.id();
                    let account_name = task_account
                        .get(&task_id)
                        .unwrap_or_else(|| unreachable!());
                    let pb = task_bar
                        .get(&task_id)
                        .unwrap_or_else(|| unreachable!());
                    let err_msg = error.to_string();
                    let panic = error.into_panic();
                    tracing::error!(
                        ?account_name,
                        ?task_id,
                        ?err_msg,
                        ?panic,
                        "Account fetch panicked."
                    );
                    prog_fin_err(account_name, &pb, &err_msg);
                }
                Err(error) => unreachable!(
                    "tokio::task::JoinError was neither panic nor cancellation:\
                    {error:?}"
                ),
            }
        }

        Ok(())
    }
}

#[tracing::instrument(name = "account", skip_all, fields(name = account_name, task_id = ?task_id))]
async fn fetch_account(
    task_id: task::Id,
    account_name: &str,
    account: &ImapAccount,
    db: &data::Storage,
    all: bool,
    pb: ProgressBar,
) -> anyhow::Result<()> {
    tracing::info!(?account, "Fetching.");
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
        let status_mailbox =
            format!("{:?} ({m} / {m_total})", truncate(&mailbox, 25));
        let status_account_mailbox = {
            let mailbox_status = console::style(&status_mailbox).dim();
            format!("{account_name:?} : {mailbox_status}")
        };
        pb.set_message(status_account_mailbox);
        let last_seen_uid = db
            .fetch_last_seen(account_name, &mailbox)
            .await?
            .unwrap_or(1);
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
                while let Some(imap::Msg {
                    uid,
                    ord: ord_curr,
                    raw,
                }) = msgs.next().await
                {
                    let subject = mail_parser::MessageParser::default()
                        .parse(&raw[..])
                        .and_then(|msg| {
                            msg.subject().map(|subj| truncate(subj, 25))
                        })
                        .unwrap_or_default();
                    let status_mailbox_msg =
                        format!("{status_mailbox}: {subject:?}");
                    let status_account_mailbox_msg = {
                        let status_mailbox_msg =
                            console::style(status_mailbox_msg).dim();
                        format!("{account_name:?} : {status_mailbox_msg}")
                    };
                    pb.set_message(status_account_mailbox_msg);
                    // TODO Batch insertions.
                    db.store_msg(&raw[..]).await?;
                    if uid > last_seen_uid {
                        db.store_last_seen(account_name, &mailbox, uid)
                            .await?;
                    }
                    pb.inc(u64::from(ord_curr - ord_prev));
                    ord_prev = ord_curr;
                }
            }
        }
    }
    Ok(())
}

const MARK_OK: &str = "V";
const MARK_ERR: &str = "X";

fn prog_fin_ok(account_name: &str, pb: &ProgressBar) {
    let mark_ok = console::style(MARK_OK).green();
    pb.finish_with_message(format!("[{mark_ok}] {account_name:?}"));
}

fn prog_fin_err(account_name: &str, pb: &ProgressBar, err_msg: &str) {
    let mark_err = console::style(MARK_ERR).red();
    let err_msg = console::style(truncate(err_msg, MAX_ERR_MSG_LEN)).red();
    pb.finish_with_message(format!(
        "[{mark_err}] {account_name:?}: {err_msg:?}",
    ));
}

fn prog_bar_spin(size: u64) -> anyhow::Result<ProgressBar> {
    let bar = ProgressBar::new(size);
    let sty = prog_sty_spin()?;
    bar.set_style(sty);
    Ok(bar)
}

macro_rules! BAR_TEMPLATE {
    () => {
        "{bar:40.green} {percent:>3}% | {human_pos:>9} / {human_len:9} | {elapsed_precise} / {duration_precise} | {eta_precise} | {prefix}"
    };
}

const BAR_CUR: &str = concat!(BAR_TEMPLATE!(), "[{spinner:.yellow}] {msg}");
const BAR_FIN: &str = concat!(BAR_TEMPLATE!(), "{msg}");

fn prog_sty_spin() -> anyhow::Result<ProgressStyle> {
    let sty = ProgressStyle::with_template(BAR_CUR)?;
    Ok(sty)
}

fn prog_sty_spin_fin() -> anyhow::Result<ProgressStyle> {
    let sty = ProgressStyle::with_template(BAR_FIN)?;
    Ok(sty)
}

fn truncate<S: AsRef<str>>(s: S, max: usize) -> String {
    let s = s.as_ref();
    // XXX Redundant iteration is cheaper than redundant allocation.
    if s.chars().count() > max {
        format!("{}...", s.chars().take(max).collect::<String>())
    } else {
        s.to_string()
    }
}
