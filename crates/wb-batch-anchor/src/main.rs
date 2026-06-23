//! `wb-batch-anchor` — local/dev binary.
//!
//! Uses a file-backed registry so the full pipeline (publish → broadcast →
//! anchor → query) works end-to-end without a running LEZ sequencer. For real
//! on-chain anchoring with proof generation, use the `wb-batch-anchor-lez`
//! binary from the `wb-lez-registry` crate (see HANDOFF.md).

use clap::Parser;
use wb_batch_anchor::{init_tracing, run, Cli};
use wb_index::FileRegistry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    let registry = FileRegistry::new(cli.registry_file.clone());
    run(cli, registry).await
}
