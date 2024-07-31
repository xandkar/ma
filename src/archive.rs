use std::{
    path::{Path, PathBuf},
    pin::Pin,
};

use futures::Stream;
use sqlx::Executor;
use tokio::fs;

use crate::cfg::Cfg;

const MIGRATION_0: &str = include_str!("../migrations/0_archive.sql");

#[derive(Debug, sqlx::FromRow)]
pub struct Msg {
    pub hash: String,
    pub raw: Vec<u8>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct Header {
    pub msg_hash: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct Body {
    pub hash: String,
    pub text: Option<String>,
}

pub struct Archive {
    pool: sqlx::Pool<sqlx::Sqlite>,
}

impl Archive {
    pub async fn connect(cfg: &Cfg) -> anyhow::Result<Self> {
        let db_file = PathBuf::from("archive").with_extension("db");
        let db_path = cfg.db_dir.join(db_file);
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

    pub async fn headers<'a>(
        &'a self,
        msg_hash: &'a str,
    ) -> Pin<Box<dyn Stream<Item = sqlx::Result<Header>> + 'a>> {
        sqlx::query_as("SELECT * FROM headers WHERE msg_hash = ?")
            .bind(msg_hash)
            .fetch(&self.pool)
    }

    pub async fn body<'a>(
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

    pub async fn insert_dumped(&self, cfg: &Cfg) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        // TODO Parallelize:
        //      - task to par_iter (rayon) from fs and write to channel
        //      - task to read from channel and write to db
        for msg in dumped(&cfg.obj_dir) {
            tx = tx_insert_msg(tx, &msg).await?;
            match mailparse::parse_headers(&msg.raw[..]) {
                Err(error) => {
                    tracing::error!(
                        msg = msg.hash,
                        ?error,
                        "Failed to parse headers."
                    );
                }
                Ok((headers, _)) => {
                    for header in headers {
                        tx = tx_insert_header(
                            tx,
                            &msg.hash,
                            &header.get_key(),
                            &header.get_value(),
                        )
                        .await?;
                    }
                }
            }

            if let Some(body_text) = mail_parser::MessageParser::default()
                .parse(&msg.raw[..])
                .and_then(|m| m.body_text(0).map(|b| b.to_string()))
            {
                tx = tx_insert_body(tx, &msg.hash, &body_text).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }
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
