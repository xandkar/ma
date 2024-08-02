use std::{
    path::{Path, PathBuf},
    pin::Pin,
};

use futures::Stream;
use sqlx::Executor;
use tokio::fs;

use crate::cfg::Cfg;

const MIGRATION_0: &str = include_str!("../migrations/0_archive.sql");

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

pub struct Archive {
    pool: sqlx::Pool<sqlx::Sqlite>,
}

impl Archive {
    pub async fn connect(db_dir: &Path) -> anyhow::Result<Self> {
        let db_file = PathBuf::from("archive").with_extension("db");
        let db_path = db_dir.join(db_file);
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(&parent).await?;
        }
        let url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        let selph = Self { pool };
        selph.pool.execute(MIGRATION_0).await?;
        Ok(selph)
    }

    pub async fn fetch_messages<'a>(
        &'a self,
    ) -> Pin<Box<dyn Stream<Item = sqlx::Result<Msg>> + 'a>> {
        sqlx::query_as("SELECT * FROM messages").fetch(&self.pool)
    }

    pub async fn fetch_headers<'a>(
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

    pub async fn store(&self, hash: &str, raw: &[u8]) -> anyhow::Result<()> {
        let msg = Msg {
            hash: hash.to_string(),
            raw: raw.to_vec(),
        };
        let mut tx = self.pool.begin().await?;
        tx = tx_insert(tx, &msg).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn insert_dumped(&self, cfg: &Cfg) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        // TODO Parallelize:
        //      - task to par_iter (rayon) from fs and write to channel
        //      - task to read from channel and write to db
        for msg in dumped(&cfg.obj_dir) {
            tx = tx_insert(tx, &msg).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn tx_insert<'tx>(
    mut tx: sqlx::Transaction<'tx, sqlx::Sqlite>,
    msg: &Msg,
) -> anyhow::Result<sqlx::Transaction<'tx, sqlx::Sqlite>> {
    tx = tx_insert_msg(tx, &msg).await?;
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

async fn tx_insert_msg<'tx>(
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

fn dumped(path: &Path) -> impl Iterator<Item = Msg> {
    crate::fs::find_files(path)
        .filter(|p| p.to_string_lossy().ends_with(".eml.gz"))
        .filter_map(|path| {
            let stem = path.file_stem().and_then(|s| {
                s.to_string_lossy()
                    .strip_suffix(".eml")
                    .map(|s| s.to_string())
            });
            stem.map(|s| (path, s))
        })
        .filter_map(|(path, stem)| {
            crate::file::read_gz(&path)
                .ok()
                .map(|raw| Msg { hash: stem, raw })
        })
        .take(5)
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use crate::hash;

    use super::*;

    #[tokio::test]
    async fn roundtrip() {
        let db_dir = tempfile::tempdir().unwrap();
        let archive = Archive::connect(db_dir.path()).await.unwrap();
        let msg: &str = "Foo: bar\nBaz: qux\n\nHi";
        let msg_hash = hash::sha256(msg);
        archive.store(&msg_hash, msg.as_bytes()).await.unwrap();

        let mut headers_actual: Vec<Header> = archive
            .fetch_headers(&msg_hash)
            .await
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
            archive.fetch_body(&msg_hash).await.unwrap().unwrap()
        );

        assert_eq!(
            vec![Msg {
                hash: msg_hash.clone(),
                raw: msg.as_bytes().to_vec(),
            }],
            archive
                .fetch_messages()
                .await
                .filter_map(|res| async { res.ok() })
                .collect::<Vec<Msg>>()
                .await
        );
    }
}
