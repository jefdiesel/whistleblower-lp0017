//! Whistleblower on-chain CID registry — a SPEL `#[lez_program]` for the Logos
//! Execution Zone (LEZ).
//!
//! Model (Solana-like, one account per document):
//! * Each registered CID lives in its own program-derived account (PDA), derived
//!   deterministically from the CID:
//!     pda = compute_pda(self_program_id, cid_seed(cid))
//!   where `cid_seed` (in `wb_registry_core`) = SHA256("WB-CID-PDA-v1" || cid).
//!   CIDs exceed the 32-byte seed limit, so they are hashed.
//! * The account data holds borsh(`RegistryRecord { cid, metadata_hash,
//!   anchor_timestamp }`).
//! * `anchor_batch` registers many CIDs in one transaction (the batch tool sends
//!   >= 10; the 50-CID path is the benchmark target).
//! * It is **idempotent**: an already-initialized PDA is passed through
//!   unchanged, so re-anchoring a known CID (or two anchorers racing on the same
//!   CID) never fails.
//! * **Query by CID** happens off-chain: derive the PDA from the CID, read the
//!   account, borsh-decode `RegistryRecord` (see `crates/wb-lez-registry`).
//!
//! No on-chain events are emitted: the LEZ v0.1.2 / SPEL stack has no event
//! mechanism (the LP-0012 `emit_event` API does not exist at this version).

#![no_main]

use nssa_core::account::{Account, Data};
use nssa_core::program::{Claim, PdaSeed};
use spel_framework::prelude::*;

risc0_zkvm::guest::entry!(main);

/// One `(cid, metadata_hash)` tuple — the instruction-argument unit.
///
/// Defined at crate root so the generated Instruction enum (also at crate root)
/// resolves it, and `#[account_type]` so it is emitted into the IDL. Its borsh
/// layout mirrors `wb_registry_core::AnchorArg` (host side) so encodings match.
/// serde derives are required because the generated Instruction enum (which
/// carries `entries: Vec<AnchorArg>`) is itself serde-(de)serializable.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, serde::Serialize, serde::Deserialize)]
#[account_type]
pub struct AnchorArg {
    pub cid: String,
    pub metadata_hash: [u8; 32],
}

#[lez_program]
mod whistleblower_registry {
    #[allow(unused_imports)]
    use super::*;

    /// Per-document account payload. Borsh layout mirrors
    /// `wb_registry_core::RegistryRecord` so the off-chain query path decodes it.
    #[derive(BorshSerialize, BorshDeserialize, Clone)]
    #[account_type]
    pub struct RegistryRecord {
        pub cid: String,
        pub metadata_hash: [u8; 32],
        pub anchor_timestamp: u64,
    }

    /// Anchor a batch of `(cid, metadata_hash)` tuples.
    ///
    /// SPEL parameter order is `ctx → #[account] params → plain args`. `records[i]`
    /// is the PDA for `entries[i].cid`. `anchor_timestamp` is supplied by the
    /// submitter and applies to every entry (the trust model — see the program
    /// README; LEZ v0.1.2 has no on-chain clock the guest can read).
    #[instruction]
    pub fn anchor_batch(
        ctx: ProgramContext,
        #[account(mut)] records: Vec<AccountWithMetadata>,
        entries: Vec<AnchorArg>,
        anchor_timestamp: u64,
    ) -> SpelResult {
        if records.len() != entries.len() {
            return Err(SpelError::custom(
                1,
                format!("records ({}) != entries ({})", records.len(), entries.len()),
            ));
        }

        // Build parallel accounts/claims vectors and emit via `execute_with_claims`
        // directly. (The macro rewrites a plain `execute(..)` call by reading a
        // private field on the elements, so we use the lowered form it targets.)
        let mut accounts: Vec<Account> = Vec::with_capacity(records.len());
        let mut claims: Vec<AutoClaim> = Vec::with_capacity(records.len());
        for (entry, acc) in entries.into_iter().zip(records.into_iter()) {
            // 1) The provided account must be the canonical PDA for this CID.
            let raw_seed = wb_registry_core::cid_seed(&entry.cid);
            let expected = spel_framework::pda::compute_pda(&ctx.self_program_id, &[&raw_seed]);
            if acc.account_id != expected {
                return Err(SpelError::PdaMismatch {
                    account_name: entry.cid.clone(),
                    expected: format!("{expected:?}"),
                    actual: format!("{:?}", acc.account_id),
                });
            }

            // 2) Idempotent: claim + write only an unowned (default) account;
            //    pass an already-registered account through unchanged (no claim).
            let mut account = acc.account;
            if account == Account::default() {
                let record = RegistryRecord {
                    cid: entry.cid,
                    metadata_hash: entry.metadata_hash,
                    anchor_timestamp,
                };
                let bytes = borsh::to_vec(&record)
                    .map_err(|e| SpelError::custom(2, format!("borsh encode failed: {e}")))?;
                account.data = Data::try_from(bytes)
                    .map_err(|_| SpelError::custom(3, "record exceeds account data limit"))?;
                accounts.push(account);
                claims.push(AutoClaim::Claimed(Claim::Pda(PdaSeed::new(raw_seed))));
            } else {
                accounts.push(account);
                claims.push(AutoClaim::None);
            }
        }

        Ok(SpelOutput::execute_with_claims(&accounts, &claims, vec![]))
    }

    /// Anchor a single (cid, metadata_hash). Scalar args so the SPEL IDL CLI can
    /// submit it (the CLI cannot encode Vec<struct>). `record` must be the PDA for `cid`.
    #[instruction]
    pub fn anchor_one(
        ctx: ProgramContext,
        #[account(mut)] record: AccountWithMetadata,
        cid: String,
        metadata_hash: [u8; 32],
        anchor_timestamp: u64,
    ) -> SpelResult {
        let raw_seed = wb_registry_core::cid_seed(&cid);
        let expected = spel_framework::pda::compute_pda(&ctx.self_program_id, &[&raw_seed]);
        if record.account_id != expected {
            return Err(SpelError::PdaMismatch {
                account_name: cid.clone(),
                expected: format!("{expected:?}"),
                actual: format!("{:?}", record.account_id),
            });
        }
        let mut account = record.account;
        let (accounts, claims) = if account == Account::default() {
            let rec = RegistryRecord { cid, metadata_hash, anchor_timestamp };
            let bytes = borsh::to_vec(&rec)
                .map_err(|e| SpelError::custom(2, format!("borsh encode failed: {e}")))?;
            account.data = Data::try_from(bytes)
                .map_err(|_| SpelError::custom(3, "record exceeds account data limit"))?;
            (vec![account], vec![AutoClaim::Claimed(Claim::Pda(PdaSeed::new(raw_seed)))])
        } else {
            (vec![account], vec![AutoClaim::None])
        };
        Ok(SpelOutput::execute_with_claims(&accounts, &claims, vec![]))
    }
}
