//! Demonstrates a real >=10-CID BATCH anchor in a single on-chain transaction
//! via [`wb_lez_registry::LezRegistry`] (the `anchor_batch` path the SPEL CLI
//! can't drive). Reads WB_SEQUENCER_URL / WB_PROGRAM_ID / WB_SIGNER_KEY from env.
//!
//!   cargo run --bin batch-demo -- 12

use wb_index::RegistryClient;
use wb_types::AnchorEntry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(12);
    // Optional CID prefix (default "zDvBatchDemo"). Pass a FRESH prefix to anchor
    // brand-new PDAs (the full write path) — useful for benchmarking, since
    // re-anchoring existing CIDs hits the cheaper idempotent passthrough.
    let prefix = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "zDvBatchDemo".to_string());

    let reg = wb_lez_registry::LezRegistry::from_env()?;

    let entries: Vec<AnchorEntry> = (0..n)
        .map(|i| AnchorEntry::new(format!("{prefix}{i:03}"), [0x40u8.wrapping_add(i as u8); 32]))
        .collect();

    println!("Anchoring {n} CIDs in ONE batch transaction...");
    let receipt = reg.anchor_batch(&entries, 1_719_300_000_000).await?;
    println!("  batch tx: {}", receipt.tx_hash);
    println!(
        "  submitted: {}  (anchored {} / already-present {})",
        n,
        receipt.anchored.len(),
        receipt.already_present.len()
    );

    // The standalone sequencer batches the mempool into a block periodically
    // (~15s), so `send_transaction` returns BEFORE the tx is in a block. Poll the
    // first CID until it appears (read-after-write), then verify the rest — they
    // share the one atomic tx, so once entry 0 lands they all have.
    use std::time::Duration;
    for attempt in 0..40u32 {
        if reg.get_by_cid(&entries[0].cid).await?.is_some() {
            break;
        }
        if attempt == 39 {
            anyhow::bail!("batch tx {} did not land within timeout", receipt.tx_hash);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // Read back EVERY CID by deriving its PDA, to prove the whole batch landed
    // (not just entry 0). One on-chain query per CID.
    let mut present = 0usize;
    for (i, e) in entries.iter().enumerate() {
        match reg.get_by_cid(&e.cid).await? {
            Some(rec) => {
                present += 1;
                // Print the first three + the last as a spot check.
                if i < 3 || i + 1 == n {
                    println!(
                        "  query {} -> ts={} hash={}",
                        rec.cid,
                        rec.anchor_timestamp,
                        hex::encode(rec.metadata_hash)
                    );
                }
            }
            None => println!("  query {} -> NOT FOUND", e.cid),
        }
    }
    println!("  VERIFIED {present}/{n} records present on-chain");
    if present != n {
        anyhow::bail!("only {present}/{n} records persisted");
    }
    Ok(())
}
