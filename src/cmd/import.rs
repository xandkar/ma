use std::path::PathBuf;

use crate::{cfg::Cfg, data::DataBase};

#[derive(clap::Args, Debug, Clone)]
pub struct Cmd {
    obj_dir: PathBuf,
}

impl Cmd {
    pub async fn run(&self, cfg: &Cfg) -> anyhow::Result<()> {
        let db = DataBase::connect(&cfg.db).await?;
        db.import(&self.obj_dir).await?;
        Ok(())
    }
}
