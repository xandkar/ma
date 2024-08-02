use crate::{cfg::Cfg, data::DataBase};

#[derive(clap::Args, Debug, Clone)]
pub struct Cmd {}

impl Cmd {
    pub async fn run(&self, cfg: &Cfg) -> anyhow::Result<()> {
        let db = DataBase::connect(&cfg.db).await?;
        db.insert_dumped(cfg).await?;
        Ok(())
    }
}
