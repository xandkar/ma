mod routes;
mod senders;

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
            Analyze::Senders { noise_threshold } => {
                senders::analyze(cfg, *noise_threshold).await?;
            }
        }
        Ok(())
    }
}

#[derive(clap::Subcommand, Debug)]
enum Analyze {
    /// [WIP] More work and ideas are needed.
    /// Build a DOT-language graph from all msg hops found in "Received"
    /// headers. Depending on the number of messages, this can generate a very
    /// large graph that is not very usefully-visible when rendered.
    Routes {
        /// Reduce the number of nodes and edges by grouping host addresses
        /// (removing subdomains and only using the first octets of IP
        /// addresses).
        reduce: bool,
    },

    /// [WIP] More work and ideas are needed.
    /// Try to figure-out which of the sender addresses is worthy of inclusion
    /// in an address book. This isn't straight forward because for any given
    /// message, any name can have any address and any address any name.
    Senders {
        /// How many alternate names can an address (or vice versa) have
        /// before we consider it noisy and ignore it?
        #[clap(short, long, default_value_t = 10)]
        noise_threshold: usize,
    },
}
