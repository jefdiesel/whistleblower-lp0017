//! `wb-batch-anchor-lez` — the REAL on-chain Whistleblower batch-anchor binary.
//!
//! Identical CLI to `wb-batch-anchor` (it reuses `wb_batch_anchor::Cli` +
//! `wb_batch_anchor::run`), but constructs the production [`LezRegistry`], so
//! `run`/`anchor`/`query` operate against a live LEZ sequencer and the deployed
//! `whistleblower_registry` SPEL program instead of a local file.
//!
//! Configuration is read from the environment by [`LezRegistry::from_env`]:
//! `WB_SEQUENCER_URL`, `WB_PROGRAM_ID`, `WB_SIGNER_KEY` (see the crate README).
//! The Delivery/Storage/topic/checkpoint flags are the shared ones from `Cli`.

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    wb_batch_anchor::init_tracing();
    let cli = wb_batch_anchor::Cli::parse();
    let reg = wb_lez_registry::LezRegistry::from_env()?;
    wb_batch_anchor::run(cli, reg).await
}
