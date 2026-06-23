//! Anchors a REAL >=N-CID batch on-chain via [`wb_lez_registry::LezRegistry`].
//!
//! Unlike a synthetic harness, this uploads N **real documents** to the live
//! Logos Storage (Codex) node, takes each one's **real content CID** and its
//! **real canonical SHA-256 metadata hash** (from `wb_types::MetadataEnvelope`),
//! anchors the whole batch in ONE transaction, then reads every record back by
//! CID to prove it persisted.
//!
//! Env: `WB_STORAGE_URL` (default `http://localhost:8080/api/storage/v1`) plus the
//! `LezRegistry::from_env` vars (`WB_SEQUENCER_URL` / `WB_PROGRAM_ID` /
//! `WB_SIGNER_KEY`).
//!
//!   cargo run --bin batch-demo -- 12 <batch-label>

use std::time::Duration;

use wb_index::{HttpStorage, RegistryClient, StorageClient};
use wb_types::{AnchorEntry, MetadataEnvelope};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(12);
    let label = std::env::args().nth(2).unwrap_or_else(|| "batch".to_string());

    let storage_url = std::env::var("WB_STORAGE_URL")
        .unwrap_or_else(|_| "http://localhost:8080/api/storage/v1".to_string());
    let storage = HttpStorage::new(&storage_url);
    let reg = wb_lez_registry::LezRegistry::from_env()?;

    // Run-unique nonce so re-runs upload fresh bytes (Codex CIDs are content
    // addressed): same content => same CID (idempotent), new content => new CID.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let anchor_ts: u64 = 1_719_300_000_000;

    println!("Uploading {n} REAL documents to Logos Storage, deriving real CIDs + metadata hashes...");
    let mut entries: Vec<AnchorEntry> = Vec::with_capacity(n);
    for i in 0..n {
        // Real document bytes -> real Codex CID.
        let content = format!(
            "WHISTLEBLOWER EVIDENCE LOG\n\
             ----------------------------------------\n\
             batch : {label}\n\
             index : {i}\n\
             nonce : {nonce}\n\n\
             Real document content. Logos Storage (Codex) computes the CID from\n\
             these bytes; the metadata hash is the canonical SHA-256 over the\n\
             envelope. Both are anchored on-chain in the whistleblower registry.\n"
        );
        let filename = format!("evidence-{label}-{i:03}.txt");
        let cid = storage
            .upload(content.as_bytes(), Some("text/plain"), Some(&filename))
            .await
            .map_err(|e| anyhow::anyhow!("storage upload #{i}: {e}"))?;

        // Real canonical metadata hash (SHA-256 over the envelope).
        let envelope = MetadataEnvelope::new(
            cid.clone(),
            format!("Evidence #{i} ({label})"),
            "Disclosed document anchored in the whistleblower registry.",
            "text/plain",
            content.len() as u64,
            anchor_ts,
            vec!["whistleblower".into(), "disclosure".into()],
        );
        let entry = envelope.anchor_entry();
        if i < 3 || i + 1 == n {
            println!(
                "  [{i:03}] CID {}   meta-hash {}…",
                entry.cid,
                &hex::encode(entry.metadata_hash)[..16]
            );
        } else if i == 3 {
            println!("  … ({} more documents)", n - 4);
        }
        entries.push(entry);
    }

    println!("Anchoring {n} CIDs in ONE batch transaction (RISC0_DEV_MODE=0)...");
    let receipt = reg.anchor_batch(&entries, anchor_ts).await?;
    println!("  batch tx: {}", receipt.tx_hash);
    println!(
        "  submitted: {n}  (anchored {} / already-present {})",
        receipt.anchored.len(),
        receipt.already_present.len()
    );

    // Wait for the block (standalone sequencer batches the mempool ~every 15s),
    // then verify EVERY record by reading it back by CID.
    for attempt in 0..40u32 {
        if reg.get_by_cid(&entries[0].cid).await?.is_some() {
            break;
        }
        if attempt == 39 {
            anyhow::bail!("batch tx {} did not land within timeout", receipt.tx_hash);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let mut present = 0usize;
    for (i, e) in entries.iter().enumerate() {
        match reg.get_by_cid(&e.cid).await? {
            Some(rec) => {
                present += 1;
                if i == 0 {
                    // One full record read back from chain — real CID, real hash.
                    println!("  read back from chain (record 0):");
                    println!("    cid       = {}", rec.cid);
                    println!("    timestamp = {}", rec.anchor_timestamp);
                    println!("    meta-hash = {}", hex::encode(rec.metadata_hash));
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
