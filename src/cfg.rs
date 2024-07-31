use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use tokio::fs;

const FILE_NAME: &str = "ma.toml";

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct ImapAccount {
    pub addr: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
}

impl std::fmt::Debug for ImapAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImapAccount")
            .field("addr", &self.addr)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("pass", &"<XXXXX>")
            .finish()
    }
}

impl Default for ImapAccount {
    fn default() -> Self {
        Self {
            addr: String::new(),
            port: 993,
            user: String::new(),
            pass: String::new(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct Imap {
    pub accounts: HashMap<String, ImapAccount>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Cfg {
    pub imap: Imap,
    pub obj_dir: PathBuf,
    pub db_dir: PathBuf,
}

impl Cfg {
    pub async fn from_file(path: &Path) -> anyhow::Result<Self> {
        async {
            tracing::debug!(file = ?path, "Reading cfg from file.");
            let data = fs::read_to_string(path).await?;
            let config: Self = toml::from_str(&data)?;
            tracing::debug!(?path, ?config, "Got user config from file.");
            anyhow::Ok(config)
        }
        .await
        .context(format!("File: {:?}", path))
    }

    pub async fn to_file(&self, path: &Path) -> anyhow::Result<()> {
        tracing::debug!(file = ?path, cfg = ?self, "Writing cfg to file.");
        let data = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        fs::write(path, data)
            .await
            .context(format!("File: {:?}", path))?;
        Ok(())
    }

    pub async fn read_or_init() -> anyhow::Result<Self> {
        let path = PathBuf::from(FILE_NAME);
        if path.try_exists()? {
            let data = std::fs::read_to_string(&path).with_context(|| {
                format!("Failed to read from path: {:?}", &path)
            })?;
            let cfg = toml::from_str(&data).with_context(|| {
                format!(
                    "Failed to parse config data which was read from: {:?}",
                    &path
                )
            })?;
            tracing::debug!(?path, ?cfg, "Got cfg from file.");
            Ok(cfg)
        } else {
            let selph: Self = Self::default();
            tracing::info!(?path, cfg = ?selph, "Path not found. Using defaults.");
            selph.to_file(&path).await?;
            Ok(selph)
        }
    }
}

impl Default for Cfg {
    fn default() -> Self {
        Self {
            imap: Imap {
                accounts: HashMap::from([(
                    "default".to_string(),
                    ImapAccount::default(),
                )]),
            },
            obj_dir: PathBuf::from("dump"),
            db_dir: PathBuf::from("db"),
        }
    }
}
