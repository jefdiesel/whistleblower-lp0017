//! Derive the base58 PDA AccountId for a CID under the deployed program.
//!
//! The PDA is `compute_pda(IMAGE_ID, [cid_seed(cid)])` where `cid_seed` is the
//! single source of truth in `wb_registry_core`. IMAGE_ID is the RISC0 image id
//! of the *currently deployed* `wb_registry.bin` — it changes whenever the guest
//! source changes, so it is hardcoded here and must be updated after each rebuild.
//!
//! Usage:
//!   cargo run --bin pda -- <cid>        (defaults to the standard test CID)

use nssa_core::program::ProgramId;

/// Image id of the deployed program (must match the rebuilt `wb_registry.bin`).
/// Updated after adding `anchor_one`.
const IMAGE_ID: ProgramId = [
    736378292, 3237769127, 3218962078, 1003268346, 3355061132, 654317770, 4171522436, 2002532608,
];

fn main() {
    let cid = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "zDvTestWhistleblowerCID0001".to_string());
    let seed = wb_registry_core::cid_seed(&cid);
    let account_id = spel_framework::pda::compute_pda(&IMAGE_ID, &[&seed]);
    println!("{account_id}");
}
