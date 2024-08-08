mod routes;

use crate::cfg::Cfg;

#[derive(clap::Args, Debug)]
pub struct Cmd {
    #[clap(subcommand)]
    analyze: Analyze,
}

impl Cmd {
    pub async fn run(&self, cfg: &Cfg) -> anyhow::Result<()> {
        match &self.analyze {
            Analyze::Routes { reduce } => {
                routes::trace(*reduce, cfg).await?;
            }
        }
        Ok(())
    }
}

#[derive(clap::Subcommand, Debug)]
enum Analyze {
    /// Build a DOT-language graph from all msg hops found in "Received"
    /// headers. Depending on the number of messages, this can generate a very
    /// large graph that is not very usefully-visible when rendered. More work
    /// and ideas are needed here.
    Routes {
        /// Reduce the number of nodes and edges by grouping host addresses
        /// (removing subdomains and only using the first octets of IP
        /// addresses).
        reduce: bool,
    },
}
