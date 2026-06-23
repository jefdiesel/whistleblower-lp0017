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

    let reg = wb_lez_registry::LezRegistry::from_env()?;

    let entries: Vec<AnchorEntry> = (0..n)
        .map(|i| AnchorEntry::new(format!("zDvBatchDemo{i:03}"), [0x40u8.wrapping_add(i as u8); 32]))
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

    // Read one back by CID to prove it landed.
    let probe = "zDvBatchDemo000";
    match reg.get_by_cid(probe).await? {
        Some(rec) => println!(
            "  query {probe} -> cid={} ts={} hash={}",
            rec.cid,
            rec.anchor_timestamp,
            hex::encode(rec.metadata_hash)
        ),
        None => println!("  query {probe} -> NOT FOUND (anchor may not have landed)"),
    }
    Ok(())
}
