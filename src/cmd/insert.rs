use crate::{archive::Archive, cfg::Cfg};

#[derive(clap::Args, Debug, Clone)]
pub struct Cmd {}

impl Cmd {
    pub async fn run(&self, cfg: &Cfg) -> anyhow::Result<()> {
        let archive = Archive::connect(cfg).await?;
        archive.insert_dumped(cfg).await?;
        Ok(())
    }
}
