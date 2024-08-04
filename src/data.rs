use std::{
    path::{Path, PathBuf},
    pin::Pin,
};

use anyhow::bail;
use futures::{Stream, StreamExt};
use sqlx::Executor;
use tokio::fs;

use crate::{cfg, file, hash};

const MIGRATIONS: [&str; 1] = [include_str!("../migrations/0_data.sql")];

#[derive(sqlx::FromRow, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Msg {
    pub hash: String,
    pub raw: Vec<u8>,
}

#[derive(sqlx::FromRow, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Header {
    pub msg_hash: String,
    pub name: String,
    pub value: String,
}

#[derive(sqlx::FromRow, Debug, PartialEq)]
pub struct Body {
    pub msg_hash: String,
    pub text: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct LastSeenMsg {
    pub account: String,
    pub mailbox: String,
    pub uid: u32,
}

pub struct Storage {
    pool: sqlx::Pool<sqlx::Sqlite>,
}

impl Storage {
    pub async fn connect(cfg: &cfg::Db) -> anyhow::Result<Self> {
        if let Some(parent) = cfg.file.parent() {
            fs::create_dir_all(&parent).await?;
        }
        let url = format!("sqlite://{}?mode=rwc", cfg.file.to_string_lossy());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        let selph = Self { pool };
        for migration in MIGRATIONS {
            selph.pool.execute(migration).await?;
        }
        Ok(selph)
    }

    pub async fn store_last_seen(
        &self,
        account: &str,
        mailbox: &str,
        uid: u32,
    ) -> anyhow::Result<()> {
        sqlx::query("INSERT OR REPLACE INTO last_seen_msg (account, mailbox, uid) VALUES (?, ?, ?)")
            .bind(account)
            .bind(mailbox)
            .bind(uid)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn fetch_last_seen(
        &self,
        account: &str,
        mailbox: &str,
    ) -> sqlx::Result<Option<u32>> {
        let result: sqlx::Result<LastSeenMsg> = sqlx::query_as(
            "SELECT * FROM last_seen_msg WHERE account = ? AND mailbox = ?",
        )
        .bind(account)
        .bind(mailbox)
        .fetch_one(&self.pool)
        .await;
        match result {
            Ok(seen) => Ok(Some(seen.uid)),
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn store_msg(&self, raw: &[u8]) -> anyhow::Result<()> {
        let hash = hash::sha256(raw);
        let msg = Msg {
            hash,
            raw: raw.to_vec(),
        };
        let mut tx = self.pool.begin().await?;
        tx = tx_insert_msg(tx, &msg).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn count_messages(&self) -> anyhow::Result<u64> {
        let (count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM messages")
                .fetch_one(&self.pool)
                .await?;
        let count = u64::try_from(count)?;
        Ok(count)
    }

    #[must_use]
    pub fn fetch_messages<'a>(
        &'a self,
    ) -> Pin<Box<dyn Stream<Item = sqlx::Result<Msg>> + 'a>> {
        sqlx::query_as("SELECT * FROM messages").fetch(&self.pool)
    }

    #[must_use]
    pub fn fetch_headers<'a>(
        &'a self,
        msg_hash: &'a str,
    ) -> Pin<Box<dyn Stream<Item = sqlx::Result<Header>> + 'a>> {
        sqlx::query_as("SELECT * FROM headers WHERE msg_hash = ?")
            .bind(msg_hash)
            .fetch(&self.pool)
    }

    pub async fn fetch_body<'a>(
        &'a self,
        msg_hash: &'a str,
    ) -> sqlx::Result<Option<Body>> {
        let mut bodies: Vec<Body> =
            sqlx::query_as("SELECT * FROM bodies WHERE msg_hash = ?")
                .bind(msg_hash)
                .fetch_all(&self.pool)
                .await?;
        debug_assert!(bodies.len() < 2);
        Ok(bodies.pop())
    }

    pub async fn import(&self, obj_dir: &Path) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        // TODO Parallelize:
        //      - task to par_iter (rayon) from fs and write to channel
        //      - task to read from channel and write to db
        let (msgs_count, msgs) = exported(obj_dir);
        let progress_bar =
            indicatif::ProgressBar::new(u64::try_from(msgs_count)?);
        let progress_style = indicatif::ProgressStyle::with_template(
            "{bar:100.green} {pos:>7} / {len:7}",
        )?;
        progress_bar.set_style(progress_style);
        progress_bar.tick();
        for msg in msgs {
            tx = tx_insert_msg(tx, &msg).await?;
            progress_bar.inc(1);
        }
        tx.commit().await?;
        progress_bar.finish();
        Ok(())
    }

    pub async fn export(&self, obj_dir: &Path) -> anyhow::Result<()> {
        if fs::try_exists(obj_dir).await? {
            if !fs::metadata(obj_dir).await?.is_dir() {
                bail!("Not a directory: {obj_dir:?}");
            }
        } else {
            fs::create_dir_all(obj_dir).await?;
        }
        let msgs_count = self.count_messages().await?;
        let mut msgs = self.fetch_messages();
        let progress_bar = indicatif::ProgressBar::new(msgs_count);
        let progress_style = indicatif::ProgressStyle::with_template(
            "{bar:100.green} {pos:>7} / {len:7}",
        )?;
        progress_bar.set_style(progress_style);
        progress_bar.tick();
        // TODO Parallelize.
        while let Some(msg_result) = msgs.next().await {
            let Msg { hash, raw } = msg_result?;
            file::write_as_gz(
                obj_dir.join(&hash[..2]).join(&hash).with_extension("eml"),
                raw,
            )?;
            progress_bar.inc(1);
        }
        progress_bar.finish();
        Ok(())
    }
}

async fn tx_insert_msg<'tx>(
    mut tx: sqlx::Transaction<'tx, sqlx::Sqlite>,
    msg: &Msg,
) -> anyhow::Result<sqlx::Transaction<'tx, sqlx::Sqlite>> {
    tx = tx_insert_msg_(tx, msg).await?;
    let (headers, _) = mailparse::parse_headers(&msg.raw[..])?;
    for header in headers {
        let name = header.get_key();
        let value = header.get_value();
        tx = tx_insert_header(tx, &msg.hash, &name, &value).await?;
    }
    if let Some(body_text) = mail_parser::MessageParser::default()
        .parse(&msg.raw[..])
        .and_then(|m| m.body_text(0).map(|b| b.to_string()))
    {
        tx = tx_insert_body(tx, &msg.hash, &body_text).await?;
    }
    Ok(tx)
}

async fn tx_insert_msg_<'tx>(
    mut tx: sqlx::Transaction<'tx, sqlx::Sqlite>,
    msg: &Msg,
) -> anyhow::Result<sqlx::Transaction<'tx, sqlx::Sqlite>> {
    sqlx::query("INSERT OR IGNORE INTO messages (hash, raw) VALUES (?, ?)")
        .bind(&msg.hash)
        .bind(&msg.raw[..])
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

async fn tx_insert_header<'tx>(
    mut tx: sqlx::Transaction<'tx, sqlx::Sqlite>,
    msg_hash: &str,
    name: &str,
    value: &str,
) -> anyhow::Result<sqlx::Transaction<'tx, sqlx::Sqlite>> {
    sqlx::query(
        "INSERT OR IGNORE INTO headers (msg_hash, name, value) VALUES (?, ?, ?)"
    )
        .bind(msg_hash)
        .bind(name)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

async fn tx_insert_body<'tx>(
    mut tx: sqlx::Transaction<'tx, sqlx::Sqlite>,
    msg_hash: &str,
    text: &str,
) -> anyhow::Result<sqlx::Transaction<'tx, sqlx::Sqlite>> {
    sqlx::query(
        "INSERT OR IGNORE INTO bodies (msg_hash, text) VALUES (?, ?)",
    )
    .bind(msg_hash)
    .bind(text)
    .execute(&mut *tx)
    .await?;
    Ok(tx)
}

fn exported(path: &Path) -> (usize, impl Iterator<Item = Msg>) {
    let paths_and_stems: Vec<(PathBuf, String)> = crate::fs::find_files(path)
        .filter(|p| p.to_string_lossy().ends_with(".eml.gz"))
        .filter_map(|path| {
            let stem = path.file_stem().and_then(|s| {
                s.to_string_lossy()
                    .strip_suffix(".eml")
                    .map(|s| s.to_string())
            });
            stem.map(|s| (path, s))
        })
        .collect();
    let n = paths_and_stems.len();
    let msgs = paths_and_stems.into_iter().filter_map(|(path, stem)| {
        crate::file::read_gz(&path)
            .ok()
            .map(|raw| Msg { hash: stem, raw })
    });
    (n, msgs)
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use crate::hash;

    use super::*;

    #[tokio::test]
    async fn roundtrip() {
        let obj_dir = tempfile::tempdir().unwrap().path().to_path_buf();
        let cfg = cfg::Db {
            file: tempfile::tempdir().unwrap().path().join("db"),
        };
        let db = Storage::connect(&cfg).await.unwrap();
        let msg: &str = "Foo: bar\nBaz: qux\n\nHi";
        let msg_hash = hash::sha256(msg);
        db.store_msg(msg.as_bytes()).await.unwrap();

        let mut headers_actual: Vec<Header> = db
            .fetch_headers(&msg_hash)
            .filter_map(|res| async { res.ok() })
            .collect()
            .await;
        let mut headers_expected = vec![
            Header {
                msg_hash: msg_hash.clone(),
                name: "Baz".to_string(),
                value: "qux".to_string(),
            },
            Header {
                msg_hash: msg_hash.clone(),
                name: "Foo".to_string(),
                value: "bar".to_string(),
            },
        ];
        headers_expected.sort();
        headers_actual.sort();
        assert_eq!(headers_expected, headers_actual);

        assert_eq!(
            Body {
                msg_hash: msg_hash.clone(),
                text: Some("Hi".to_string())
            },
            db.fetch_body(&msg_hash).await.unwrap().unwrap()
        );

        assert_eq!(
            vec![Msg {
                hash: msg_hash.clone(),
                raw: msg.as_bytes().to_vec(),
            }],
            db.fetch_messages()
                .filter_map(|res| async { res.ok() })
                .collect::<Vec<Msg>>()
                .await
        );

        db.export(&obj_dir).await.unwrap();
        let obj_file = format!(
            "{}.eml.gz",
            obj_dir
                .join(msg_hash[..2].to_string())
                .join(msg_hash)
                .to_string_lossy()
        );
        assert!(fs::try_exists(&obj_file).await.unwrap());

        let obj_bytes = file::read_gz(&obj_file).unwrap();
        assert_eq!(msg.as_bytes(), obj_bytes);

        let account = "foo";
        let mailbox = "bar";
        let uid: u32 = 1;
        db.store_last_seen(account, mailbox, uid).await.unwrap();
        assert_eq!(
            uid,
            db.fetch_last_seen(account, mailbox).await.unwrap().unwrap()
        );
    }
}
