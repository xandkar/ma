use std::path::{Path, PathBuf};

use sqlx::Executor;
use tokio::fs;

const MIGRATION_0: &str = include_str!("../migrations/0_state.sql");

#[derive(Debug, sqlx::FromRow)]
pub struct LastSeenMsg {
    pub account: String,
    pub mailbox: String,
    pub uid: u32,
}

pub struct State {
    pool: sqlx::Pool<sqlx::Sqlite>,
}

impl State {
    pub async fn connect(db_dir: &Path) -> anyhow::Result<Self> {
        let db_file = PathBuf::from("state").with_extension("db");
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

    pub async fn set(
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

    pub async fn get(
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
}
