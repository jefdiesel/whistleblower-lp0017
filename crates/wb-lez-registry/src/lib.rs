//! # wb-lez-registry — the real on-chain [`RegistryClient`] for Whistleblower
//!
//! [`LezRegistry`] is the production implementation of
//! [`wb_index::RegistryClient`]. It talks to a Logos Execution Zone (LEZ)
//! sequencer over JSON-RPC, submitting transactions that invoke the
//! `whistleblower_registry` SPEL program's `anchor_batch` instruction, and reads
//! back per-CID program-derived accounts (PDAs) to answer queries.
//!
//! The rest of the Whistleblower codebase never depends on this crate (and thus
//! never pulls the heavy RISC0/LEZ build graph): the generic
//! [`wb_index::RegistryClient`] trait is the seam, the file/mock registries cover
//! local + CI use, and this crate is wired in only by the `wb-batch-anchor-lez`
//! binary on a provisioned build machine.
//!
//! ## Encoding contract with the on-chain program
//!
//! The program (in `crates/wb-registry-program`) declares roughly:
//!
//! ```ignore
//! #[lez_program]
//! mod whistleblower_registry {
//!     fn anchor_batch(
//!         ctx,
//!         #[account(mut)] records: Vec<AccountWithMetadata>,
//!         entries: Vec<AnchorArg /* { cid: String, metadata_hash: [u8;32] } */>,
//!         anchor_timestamp: u64,
//!     );
//!     fn anchor_one(ctx, #[account(mut)] record, cid, metadata_hash, anchor_timestamp);
//! }
//! ```
//!
//! and shared types live in `wb_registry_core` (`RegistryRecord`, `AnchorArg`,
//! and `cid_seed(cid) = SHA256(b"WB-CID-PDA-v1" || cid)`).
//!
//! Each document's account is the PDA
//! `AccountId::for_public_pda(&program_id, &PdaSeed::new(cid_seed(cid)))`, whose
//! data is `borsh(RegistryRecord)`.
//!
//! ## How the instruction is encoded (VERIFIED against LEZ v0.1.2 + SPEL)
//!
//! Verified by reading, at the pinned sources:
//!   * LEZ v0.1.2: `nssa/src/public_transaction/{message,witness_set,transaction}.rs`,
//!     `nssa/src/program.rs` (`Program::serialize_instruction`), `nssa/src/lib.rs`,
//!     `nssa_core/src/{program,account,account/data}.rs`,
//!     `sequencer/service/rpc/src/lib.rs`, `common/src/transaction.rs`,
//!     `common/src/lib.rs`.
//!   * SPEL (`github.com/logos-co/spel`, branch `main`):
//!     `spel-client-gen/src/codegen.rs` — the *generated* typed client, which is
//!     the authoritative reference for how to encode + submit an instruction.
//!
//! The important facts:
//!
//! 1. **The instruction is a `serde`-serializable VALUE, not bytes, and it is
//!    serialized by `Message::try_new` (NOT by us).** `Message::try_new<T:
//!    Serialize>(program_id, account_ids, nonces, instruction)` calls
//!    `Program::serialize_instruction(instruction)` internally, which is
//!    `risc0_zkvm::serde::to_vec(&instruction)` producing a `Vec<u32>`
//!    (`InstructionData = Vec<u32>`). The guest reads it back with
//!    `risc0_zkvm::serde::Deserializer`. So the on-chain wire format is **risc0
//!    word-serde, NOT borsh**, and we must hand `try_new` a *typed value*, never
//!    pre-serialized bytes.
//!
//! 2. **The "instruction selector" is the SPEL `Instruction` enum discriminant.**
//!    `spel-client-gen` generates
//!    `enum WhistleblowerRegistryInstruction { AnchorBatch { entries, anchor_timestamp }, AnchorOne { .. } }`
//!    (`#[derive(Serialize, Deserialize)]`, variants in IDL/declaration order) and
//!    passes `Instruction::AnchorBatch { .. }` straight to `Message::try_new`. The
//!    risc0-serde encoding of that enum (a u32 variant index followed by the
//!    fields) IS the selector + args. There is **no separate manual prefix**.
//!    [`encode_anchor_batch_instruction`] mirrors that generated variant exactly.
//!
//! ### PREFER the SPEL-generated typed client
//!
//! [`encode_anchor_batch_instruction`] hand-mirrors one variant of the generated
//! `WhistleblowerRegistryInstruction` enum. That is correct **iff** our local
//! `AnchorBatchInstruction` enum stays byte-identical to the generated one
//! (variant *order* — `anchor_batch` must remain the first instruction so its
//! discriminant is 0 — field order, and `AnchorArg`'s serde layout). The robust
//! path is to generate the client and call its `anchor_batch(..)` builder:
//!
//! ```text
//! spel-client-gen --idl whistleblower_registry-idl.json --target rust+ffi --out <crate>
//! ```
//!
//! then depend on the generated crate and replace [`encode_anchor_batch_instruction`]
//! + the `Message`/`WitnessSet`/`PublicTransaction` assembly in `anchor_batch`
//! with the generated `WhistleblowerRegistryClient::anchor_batch(..)` call. See the
//! README ("Generate the SPEL typed client").

#![allow(async_fn_in_trait)]

use std::env;

use anyhow::{anyhow, Context};

// --- LEZ v0.1.2 imports --------------------------------------------------------
// VERIFIED (sequencer/service/rpc/src/lib.rs): with the `client` feature, this
// crate re-exports `HttpClientBuilder as SequencerClientBuilder`, the concrete
// `SequencerClient = jsonrpsee::http_client::HttpClient`, and `ClientError`. The
// RPC methods live on the `#[rpc(client)]`-generated extension trait `RpcClient`
// (jsonrpsee names it `<Trait>Client`, and the trait here is `Rpc`); importing it
// with `as _` brings `send_transaction` / `get_account` / `get_accounts_nonces`
// into scope on the client.
use sequencer_service_rpc::{RpcClient as _, SequencerClient, SequencerClientBuilder};

// VERIFIED. `ProgramId` (= `[u32; 8]`) and `PdaSeed` live in `nssa_core::program`;
// `AccountId` (with the inherent `for_public_pda`) lives in `nssa_core::account`.
// `nssa` re-exports `{AccountId, ProgramId}` and `program::ProgramId`, but we take
// them straight from `nssa_core` since `PdaSeed` is only exposed there.
use nssa_core::account::AccountId;
use nssa_core::program::{PdaSeed, ProgramId};

// VERIFIED. The public, program-invoking transaction is assembled from
// `nssa::public_transaction::{Message, WitnessSet}` + `nssa::PublicTransaction`,
// then wrapped in `common::transaction::NSSATransaction::Public(..)` for
// `send_transaction`. (There is no `LeeTransaction` at v0.1.2 — the on-wire enum
// is `NSSATransaction`.) The signing key is `nssa::PrivateKey` (k256/BIP-340
// schnorr), and `common::HashType` is what `send_transaction` returns.
//
// NOTE: this requires the `nssa` git dependency (pinned to the SAME tag) — added
// to Cargo.toml alongside `common`/`nssa_core`/`sequencer_service_rpc`.
use common::transaction::NSSATransaction;
use nssa::public_transaction::{Message, WitnessSet};
use nssa::{PrivateKey, PublicTransaction};

/// Shared on-chain types (written in parallel by the program crate). We use
/// `AnchorArg` for instruction encoding, `RegistryRecord` for decoding account
/// data, and `cid_seed` for PDA derivation — guaranteeing host/guest agreement.
///
/// `AnchorArg` derives `serde::{Serialize, Deserialize}` (verified in
/// `wb_registry_core`), so it can ride directly inside the instruction enum that
/// `Message::try_new` serializes with risc0 word-serde.
use wb_registry_core::{cid_seed, AnchorArg, RegistryRecord as ProgramRecord};

use wb_index::{AnchorReceipt, RegistryError};
use wb_types::{AnchorEntry, RegistryRecord};

/// Default sequencer JSON-RPC endpoint (overridable via `WB_SEQUENCER_URL`).
pub const DEFAULT_SEQUENCER_URL: &str = "http://localhost:3040";

/// The signing key type used to authorize transactions.
///
/// VERIFIED: `nssa::PrivateKey` (re-exported from `nssa::signature`). It wraps a
/// 32-byte k256 secret key and is constructed via `PrivateKey::try_new([u8; 32])`
/// or `FromStr` (hex). `WitnessSet::for_message` borrows `&[&PrivateKey]`.
pub type SignerKey = PrivateKey;

/// The concrete sequencer client.
///
/// VERIFIED: `SequencerClientBuilder::build` returns
/// `sequencer_service_rpc::SequencerClient` (= `jsonrpsee::http_client::HttpClient`).
/// It is cheap to clone and `Send + Sync`.
type Client = SequencerClient;

/// On-chain [`RegistryClient`](wb_index::RegistryClient) backed by a LEZ
/// sequencer and the `whistleblower_registry` SPEL program.
pub struct LezRegistry {
    client: Client,
    program_id: ProgramId,
    signer: SignerKey,
}

impl LezRegistry {
    /// Construct from explicit components.
    ///
    /// `sequencer_url` is the JSON-RPC endpoint (e.g. `http://localhost:3040`).
    /// `program_id` is the deployed `whistleblower_registry` program id.
    /// `signer` authorizes the anchoring transactions (any keypair: anchoring is
    /// permissionless and idempotent on-chain).
    pub fn new(
        sequencer_url: &str,
        program_id: ProgramId,
        signer: SignerKey,
    ) -> anyhow::Result<Self> {
        // VERIFIED (rpc docs example): `SequencerClientBuilder::default().build(url)`
        // where `url` is a parsed `jsonrpsee` URL (the builder is jsonrpsee's
        // `HttpClientBuilder`, whose `build` accepts `impl AsRef<str>`; passing the
        // raw &str also works, but we parse first to fail fast on a bad URL).
        let client = SequencerClientBuilder::default()
            .build(sequencer_url)
            .map_err(|e| anyhow!("building sequencer client for {sequencer_url:?}: {e}"))?;

        Ok(Self {
            client,
            program_id,
            signer,
        })
    }

    /// Construct from the environment:
    ///
    /// * `WB_SEQUENCER_URL` — sequencer JSON-RPC endpoint
    ///   (default [`DEFAULT_SEQUENCER_URL`]).
    /// * `WB_PROGRAM_ID` — the deployed program id (see [`parse_program_id`]).
    /// * `WB_SIGNER_KEY` — the signing key (see [`parse_signer_key`]).
    pub fn from_env() -> anyhow::Result<Self> {
        let sequencer_url =
            env::var("WB_SEQUENCER_URL").unwrap_or_else(|_| DEFAULT_SEQUENCER_URL.to_string());

        let program_id_raw = env::var("WB_PROGRAM_ID")
            .context("WB_PROGRAM_ID must be set (the deployed whistleblower_registry program id)")?;
        let program_id = parse_program_id(&program_id_raw)
            .with_context(|| format!("parsing WB_PROGRAM_ID={program_id_raw:?}"))?;

        let signer_raw = env::var("WB_SIGNER_KEY")
            .context("WB_SIGNER_KEY must be set (the transaction signing key)")?;
        let signer = parse_signer_key(&signer_raw).context("parsing WB_SIGNER_KEY")?;

        Self::new(&sequencer_url, program_id, signer)
    }

    /// Derive the PDA [`AccountId`] for a CID's registry record.
    ///
    /// Mirrors the program exactly:
    /// `AccountId::for_public_pda(&program_id, &PdaSeed::new(cid_seed(cid)))`.
    ///
    /// VERIFIED (`nssa_core::program`): `for_public_pda(program_id: &ProgramId,
    /// seed: &PdaSeed)` and `PdaSeed::new([u8; 32])`. The guest derives the same
    /// PDA via `spel_framework::pda::compute_pda(&self_program_id, &[&cid_seed])`,
    /// which is the SPEL wrapper over this exact `for_public_pda` formula.
    fn pda_for_cid(&self, cid: &str) -> AccountId {
        let seed = PdaSeed::new(cid_seed(cid));
        AccountId::for_public_pda(&self.program_id, &seed)
    }
}

impl wb_index::RegistryClient for LezRegistry {
    async fn anchor_batch(
        &self,
        entries: &[AnchorEntry],
        anchor_timestamp_ms: u64,
    ) -> Result<AnchorReceipt, RegistryError> {
        if entries.is_empty() {
            // Nothing to submit; report an empty, successful no-op.
            return Ok(AnchorReceipt {
                tx_hash: String::new(),
                anchored: Vec::new(),
                already_present: Vec::new(),
            });
        }

        // 1) Derive the writable PDA account for every entry, in input order.
        //    The program's `#[account(mut)] records: Vec<..>` (IDL `rest: true`)
        //    is consumed positionally against `entries`, so `records[i]` must be
        //    the PDA for `entries[i].cid`.
        let account_ids: Vec<AccountId> =
            entries.iter().map(|e| self.pda_for_cid(&e.cid)).collect();

        // 2) Determine the signer/nonce set.
        //
        //    VERIFIED, with a caveat. In the SPEL-generated client, `nonces`
        //    correspond to the *signer* accounts (those with `signer: true` in the
        //    IDL), and `Message.account_ids` (the writable PDAs) are a SEPARATE
        //    field. The `whistleblower_registry` IDL declares its `records` account
        //    with `signer: false` and has NO signer account, so the generated
        //    client would build an EMPTY signer set → empty `nonces` → an unsigned
        //    witness. Anchoring is permissionless on-chain, which is consistent
        //    with that.
        //
        //    This adapter, however, is wired with an explicit fee-payer `signer`
        //    (`WB_SIGNER_KEY`). We sign with it and supply that signer's nonce, so
        //    the message is authenticated end-to-end. The signer's own account id
        //    is `AccountId::from(&PublicKey)`, derived inside `WitnessSet`; its
        //    nonce is fetched here.
        //
        //    TODO(verify against the DEPLOYED sequencer policy): whether v0.1.2's
        //    sequencer REQUIRES a fee-payer signature for a permissionless program
        //    invocation (sign + 1 nonce, as below) or ACCEPTS an unsigned tx with
        //    empty nonces (matching the generated client for this signer-less IDL).
        //    Both `Message` shapes are valid borsh; only the live mempool policy
        //    decides. If unsigned is required/accepted, set `nonces = vec![]` and
        //    `signing_keys = &[]`. This is the single remaining unverifiable spot.
        let signer_account_id = AccountId::from(&nssa::PublicKey::new_from_private_key(&self.signer));
        let nonces = self
            .client
            // VERIFIED: `get_accounts_nonces(account_ids: Vec<AccountId>) ->
            // Result<Vec<Nonce>, ClientError>` — takes an OWNED Vec, returns
            // nonces aligned with the input order.
            .get_accounts_nonces(vec![signer_account_id])
            .await
            .map_err(|e| RegistryError::Transport(format!("get_accounts_nonces: {e}")))?;

        // 3) Build the typed instruction value (the SPEL `Instruction` enum
        //    variant). NOT pre-serialized: `Message::try_new` serializes it with
        //    risc0 word-serde. See [`encode_anchor_batch_instruction`].
        let instruction = encode_anchor_batch_instruction(entries, anchor_timestamp_ms);

        // 4) Build the message: program_id (by value) + writable accounts +
        //    signer nonces + the typed instruction.
        //    VERIFIED: `Message::try_new<T: Serialize>(program_id: ProgramId,
        //    account_ids: Vec<AccountId>, nonces: Vec<Nonce>, instruction: T)
        //    -> Result<Message, NssaError>`.
        let message = Message::try_new(self.program_id, account_ids, nonces, instruction)
            .map_err(|e| RegistryError::Rejected(format!("Message::try_new: {e}")))?;

        // 5) Sign the message -> witness set.
        //    VERIFIED: `WitnessSet::for_message(message: &Message,
        //    private_keys: &[&PrivateKey]) -> WitnessSet` — INFALLIBLE (returns
        //    `Self`, not `Result`) and borrows a slice of `&PrivateKey`.
        let witness_set = WitnessSet::for_message(&message, &[&self.signer]);

        // 6) Wrap into a public transaction and submit.
        //    VERIFIED: `PublicTransaction::new(message, witness_set)` then
        //    `send_transaction(NSSATransaction::Public(tx)) -> Result<HashType, _>`.
        let tx = NSSATransaction::Public(PublicTransaction::new(message, witness_set));

        let tx_hash = self
            .client
            .send_transaction(tx)
            .await
            .map_err(map_send_error)?;
        // VERIFIED: `HashType(pub [u8; 32])` whose `Display` is lowercase hex; the
        // generated client uses `hex::encode(response.0)`. `to_string()` yields the
        // identical hex string, so we use it (no field access needed).
        let tx_hash = tx_hash.to_string();

        // 7) Build the receipt.
        //
        // The on-chain program is idempotent: anchoring a CID that already exists
        // is a safe no-op, so re-submitting a batch never fails. That makes the
        // permissionless batch-anchor loop crash-safe. However, the sequencer
        // submit response does not, by itself, tell us which CIDs were *newly*
        // written versus already present. Computing a precise
        // anchored-vs-already_present split would require reading each PDA back
        // (e.g. calling `get_by_cid` before/after, or having the program emit per-
        // entry events; v0.1.2/SPEL has no event mechanism). We deliberately do
        // NOT pay that round-trip cost on the hot path: we report every submitted
        // CID under `anchored` and leave `already_present` empty. Downstream logic
        // treats both as "anchored".
        let anchored: Vec<String> = entries.iter().map(|e| e.cid.clone()).collect();

        Ok(AnchorReceipt {
            tx_hash,
            anchored,
            already_present: Vec::new(),
        })
    }

    async fn get_by_cid(&self, cid: &str) -> Result<Option<RegistryRecord>, RegistryError> {
        let pda = self.pda_for_cid(cid);

        // VERIFIED: `get_account(account_id: AccountId) -> Result<Account,
        // ClientError>` — takes the id BY VALUE and returns a NON-optional
        // `Account`. An unwritten/absent account comes back as `Account::default()`
        // whose `data` is empty, so "absent" is normalized to `None` below. (This
        // mirrors the generated `fetch_*` helper, which does
        // `get_account(id).await?` then `T::try_from_slice(&account.data)`.)
        let account = self
            .client
            .get_account(pda)
            .await
            .map_err(|e| RegistryError::Transport(format!("get_account: {e}")))?;

        // `Account.data` is `nssa_core::account::Data`, which derefs to `&[u8]`.
        // Empty data == never written == not anchored.
        let data: &[u8] = &account.data;
        if data.is_empty() {
            return Ok(None);
        }

        // Decode the program's borsh-encoded RegistryRecord and convert into the
        // shared `wb_types::RegistryRecord`. Decoding `&[u8]` (not a reader)
        // rejects trailing bytes, surfacing ABI drift early.
        //
        // NOTE: the *account data* is borsh (the guest writes `borsh::to_vec(&record)`
        // into `account.data`). That is independent of the *instruction* encoding,
        // which is risc0 word-serde — do not conflate the two.
        let program_record = ProgramRecord::try_from_slice(data)
            .map_err(|e| RegistryError::Decode(format!("borsh-decoding RegistryRecord: {e}")))?;

        Ok(Some(into_wb_record(program_record)))
    }
}

/// Build the typed `anchor_batch` instruction VALUE to hand to
/// [`Message::try_new`] (which serializes it with risc0 word-serde — see crate
/// docs). This mirrors the SPEL-generated `WhistleblowerRegistryInstruction`
/// enum's first variant exactly.
///
/// PREFER the generated typed client over this hand-mirror: the correctness of
/// the discriminant depends entirely on [`AnchorBatchInstruction`] staying
/// variant-for-variant and field-for-field identical to the generated enum (in
/// particular `anchor_batch` must remain the FIRST variant so risc0-serde encodes
/// its discriminant as 0). See [`AnchorBatchInstruction`].
fn encode_anchor_batch_instruction(
    entries: &[AnchorEntry],
    anchor_timestamp_ms: u64,
) -> AnchorBatchInstruction {
    // Map the host's AnchorEntry -> the program's AnchorArg so the on-chain serde
    // layout is authoritative (single source of truth in wb_registry_core).
    let args: Vec<AnchorArg> = entries
        .iter()
        .map(|e| AnchorArg {
            cid: e.cid.clone(),
            metadata_hash: e.metadata_hash,
        })
        .collect();

    AnchorBatchInstruction::AnchorBatch {
        entries: args,
        anchor_timestamp: anchor_timestamp_ms,
    }
}

/// Local mirror of the SPEL-generated `WhistleblowerRegistryInstruction` enum.
///
/// `spel-client-gen` (verified in `spel-client-gen/src/codegen.rs`) emits, for
/// this program:
///
/// ```ignore
/// #[derive(Clone, Debug, Serialize, Deserialize)]
/// pub enum WhistleblowerRegistryInstruction {
///     AnchorBatch { entries: Vec<AnchorArg>, anchor_timestamp: u64 },
///     AnchorOne   { cid: String, metadata_hash: [u8; 32], anchor_timestamp: u64 },
/// }
/// ```
///
/// and passes the chosen variant straight to `Message::try_new`. risc0 word-serde
/// encodes the enum as a `u32` variant index (the "selector") followed by the
/// variant's fields. The guest deserializes the same `Instruction` enum, so the
/// variant ORDER is the contract.
///
/// We only need to *send* `anchor_batch`, but we keep both variants (and their
/// declaration order: `anchor_batch` first, `anchor_one` second — matching the
/// guest source `methods/guest/src/bin/wb_registry.rs`) so the discriminant of
/// `AnchorBatch` is 0, exactly as the generated enum produces.
///
/// TODO(verify against the *checked-in* IDL / a fresh `spel-client-gen` run):
/// `crates/wb-registry-program/whistleblower_registry-idl.json` currently lists
/// ONLY `anchor_batch` (it predates `anchor_one`). If the deployed program's IDL
/// truly exposes a single instruction, `AnchorBatch` is the sole variant and its
/// discriminant is still 0 — so this remains correct either way. But if the IDL
/// is regenerated to expose `anchor_one` BEFORE `anchor_batch`, the discriminant
/// shifts; regenerate the typed client and drop this mirror. (Prefer the generated
/// client to make this impossible to get wrong.)
#[derive(serde::Serialize)]
enum AnchorBatchInstruction {
    // Discriminant 0 — the SPEL `Instruction::AnchorBatch` variant.
    AnchorBatch {
        entries: Vec<AnchorArg>,
        anchor_timestamp: u64,
    },
    // Discriminant 1 — kept ONLY to fix the variant index/order to match the
    // generated enum. Never constructed here (we never submit `anchor_one`), hence
    // the allow.
    #[allow(dead_code)]
    AnchorOne {
        cid: String,
        metadata_hash: [u8; 32],
        anchor_timestamp: u64,
    },
}

/// Convert the program's `RegistryRecord` into the shared `wb_types` one.
///
/// They are intended to be field-identical (`cid`, `metadata_hash`,
/// `anchor_timestamp`); this explicit mapping localizes any future divergence.
fn into_wb_record(r: ProgramRecord) -> RegistryRecord {
    RegistryRecord {
        cid: r.cid,
        metadata_hash: r.metadata_hash,
        anchor_timestamp: r.anchor_timestamp,
    }
}

/// Map a `send_transaction` error into the right [`RegistryError`].
///
/// `send_transaction` yields `sequencer_service_rpc::ClientError` (jsonrpsee).
/// Rather than match its variants (which mix transport, encoding, and
/// server-side `ErrorObject`s), we classify on the rendered message: anything
/// that reads like a validation/rejection (the chain said "no") becomes
/// [`RegistryError::Rejected`]; everything else (connection, timeout, RPC layer)
/// becomes [`RegistryError::Transport`] and is retryable.
///
/// TODO(refine, optional): for sharper classification, match
/// `ClientError::Call(ErrorObject)` (a server-side rejection with a JSON-RPC
/// error code) vs. the transport variants (`ClientError::Transport`,
/// `RequestTimeout`, etc.). The string heuristic is a safe default; the variant
/// match is a nicety, not a correctness requirement.
fn map_send_error(e: sequencer_service_rpc::ClientError) -> RegistryError {
    let msg = e.to_string();
    let lower = msg.to_ascii_lowercase();
    let looks_rejected = lower.contains("reject")
        || lower.contains("invalid")
        || lower.contains("nonce")
        || lower.contains("insufficient")
        || lower.contains("verification")
        || lower.contains("malformed")
        || lower.contains("signature")
        || lower.contains("denied");
    if looks_rejected {
        RegistryError::Rejected(msg)
    } else {
        RegistryError::Transport(msg)
    }
}

/// Parse `WB_PROGRAM_ID` into a [`ProgramId`] (`[u32; 8]`).
///
/// Accepted forms (in order of attempt):
///   1. 64-hex chars (32 bytes) -> 8 big-endian u32 words. This is the natural
///      hex rendering of a program id and the recommended form.
///   2. 8 comma-separated decimal/hex u32 words, e.g. `1,2,3,4,5,6,7,8` or
///      `0x1,0x2,...` (handy for copy-paste from tooling that prints the words).
///
/// VERIFIED (`nssa_core::program`): `pub type ProgramId = [u32; 8]`. There is no
/// dedicated constructor / `FromStr`, so a `[u32; 8]` literal IS a `ProgramId`.
/// We return the array directly.
///
/// NOTE on byte order: a `ProgramId` is interpreted as bytes via
/// `bytemuck::cast_slice(&[u32; 8])` inside `AccountId::for_public_pda`, i.e. the
/// platform-native (little-endian on supported targets) byte layout of the words.
/// The hex form below packs hex into words big-endian, which is fine **as long as
/// the same convention is used when the deployed program id is exported to hex**.
/// If `wallet`/tooling prints the program id some other way (e.g. base58 of the
/// raw little-endian bytes), pass the comma-separated `u32` words form instead, or
/// adjust this packer to match that tool. The words form is unambiguous and
/// recommended.
pub fn parse_program_id(s: &str) -> anyhow::Result<ProgramId> {
    let s = s.trim();
    let words: [u32; 8] = if s.contains(',') {
        let parsed: Result<Vec<u32>, _> = s
            .split(',')
            .map(|w| {
                let w = w.trim();
                if let Some(hexpart) = w.strip_prefix("0x").or_else(|| w.strip_prefix("0X")) {
                    u32::from_str_radix(hexpart, 16)
                } else {
                    w.parse::<u32>()
                }
            })
            .collect();
        let v = parsed.context("WB_PROGRAM_ID: could not parse comma-separated u32 words")?;
        v.try_into().map_err(|v: Vec<u32>| {
            anyhow!("WB_PROGRAM_ID: expected 8 u32 words, got {}", v.len())
        })?
    } else {
        let hexstr = s
            .strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .unwrap_or(s);
        let bytes = hex::decode(hexstr)
            .context("WB_PROGRAM_ID: not valid hex (expected 64 hex chars / 32 bytes)")?;
        if bytes.len() != 32 {
            return Err(anyhow!(
                "WB_PROGRAM_ID: expected 32 bytes (64 hex chars), got {} bytes",
                bytes.len()
            ));
        }
        let mut words = [0u32; 8];
        for (i, chunk) in bytes.chunks_exact(4).enumerate() {
            words[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        words
    };

    // `ProgramId` is a type alias for `[u32; 8]`, so the array IS the value.
    Ok(words)
}

/// Parse `WB_SIGNER_KEY` into a [`SignerKey`] (`nssa::PrivateKey`).
///
/// Expected form: hex-encoded 32-byte k256 secret (BIP-340 schnorr) key, with or
/// without a `0x` prefix.
///
/// VERIFIED (`nssa/src/signature/private_key.rs`): `PrivateKey::try_new([u8; 32])
/// -> Result<Self, NssaError>` validates the scalar (`k256::SecretKey::from_bytes`)
/// and `impl FromStr` decodes exactly 32 hex bytes. We decode/validate explicitly
/// for precise error messages, then construct via `try_new`.
pub fn parse_signer_key(s: &str) -> anyhow::Result<SignerKey> {
    let s = s.trim();
    let hexstr = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    let bytes =
        hex::decode(hexstr).context("WB_SIGNER_KEY: expected hex-encoded 32-byte raw key")?;
    let arr: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
        anyhow!(
            "WB_SIGNER_KEY: expected 32 key bytes (64 hex chars), got {} bytes",
            bytes.len()
        )
    })?;
    PrivateKey::try_new(arr)
        .map_err(|e| anyhow!("WB_SIGNER_KEY: not a valid k256/schnorr secret key: {e}"))
}
