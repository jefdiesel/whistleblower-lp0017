# wb-lez-registry

The **real on-chain** [`RegistryClient`](../wb-index/src/registry.rs)
implementation for Whistleblower. It anchors `(CID, metadata_hash)` tuples into
the `whistleblower_registry` SPEL program on a Logos Execution Zone (LEZ)
sequencer, and reads per-CID program-derived accounts (PDAs) back to answer
queries.

It ships:

- a library (`wb_lez_registry`) exposing `LezRegistry`, and
- a binary (`wb-batch-anchor-lez`) — the on-chain twin of `wb-batch-anchor`,
  with the **same subcommands**.

Dual-licensed under **MIT OR Apache-2.0**.

> [!IMPORTANT]
> **This crate cannot be built on a machine without the RISC0 toolchain and
> network access to the LEZ git dependencies.** It pins the LEZ crates at tag
> `v0.1.2` (which transitively pull a riscv32 zkVM toolchain via the SPEL/RISC0
> program client). For exactly this reason it is its **own Cargo workspace** and
> is `exclude`d from the repo-root workspace, so the rest of the repo builds and
> tests anywhere. For local development / CI without a sequencer, use
> `wb-batch-anchor` (file-backed registry) instead.

---

## Prerequisites

1. **Rust toolchain** matching the repo (`rust-toolchain.toml`, currently
   `1.94.0`).

2. **RISC0 toolchain** (for the LEZ/zkVM build graph):

   ```sh
   curl -L https://risczero.com/install | bash
   rzup install
   ```

3. **Apply the `ring` / riscv32 patch (LEZ #468).** The `[patch.…]` block at the
   bottom of `Cargo.toml` is shipped commented out. On the build machine:

   - clone the LEZ repo and check out the tag:

     ```sh
     git clone https://github.com/logos-blockchain/logos-execution-zone \
       ../../vendor/logos-execution-zone
     git -C ../../vendor/logos-execution-zone checkout v0.1.2
     ```

   - in that checkout, set `risc0-zkvm` to `default-features = false` everywhere
     it appears (this removes the `ring`-backed crypto path that fails to build
     for `riscv32im-risc0-zkvm-elf`), re-enabling only the features you need on
     the host;
   - **uncomment** the `[patch."https://github.com/logos-blockchain/logos-execution-zone.git"]`
     block in `Cargo.toml` and adjust the relative paths to your checkout.

   (For a host-only client the break may not trigger, but the patch is ready so
   the build machine only has to flip it on.)

4. **Generate the SPEL typed client (recommended).** The hand-written
   instruction encoding in `src/lib.rs` is a *fallback*. The robust path is to
   generate a typed client from the program's IDL, which encodes the exact
   instruction selector + argument layout for you:

   ```sh
   # In the program crate (crates/wb-registry-program):
   make idl                # produces the whistleblower_registry IDL/ABI
   spel-client-gen \
     --idl <path/to/whistleblower_registry.idl> \
     --out-dir generated \
     --target rust+ffi
   ```

   Then depend on the generated crate and swap `LezRegistry`'s
   `encode_instruction` (and, ideally, the whole `Message` build) for the
   generated `anchor_batch(...)` builder. See the "Two ways to encode the
   instruction" note in `src/lib.rs`.

---

## Environment variables

| Variable           | Required | Default                  | Meaning                                                                 |
| ------------------ | -------- | ------------------------ | ----------------------------------------------------------------------- |
| `WB_SEQUENCER_URL` | no       | `http://localhost:3040`  | LEZ sequencer JSON-RPC endpoint.                                        |
| `WB_PROGRAM_ID`    | **yes**  | —                        | Deployed `whistleblower_registry` program id. 64 hex chars (32 bytes), or 8 comma-separated `u32` words. |
| `WB_SIGNER_KEY`    | **yes**  | —                        | Hex-encoded raw signing key that authorizes the anchoring transactions. |

The Delivery/Storage/topic/checkpoint settings are the **shared** flags/env from
`wb-batch-anchor` (`WB_DELIVERY_URL`, `WB_STORAGE_URL`, `WB_TOPIC`,
`WB_CHECKPOINT`); `WB_REGISTRY_FILE` is ignored by this binary (it goes
on-chain, not to a file).

> [!NOTE]
> The signer is "permissioned" only in that it authorizes/pays for the
> transaction. Anchoring is **permissionless and idempotent** on-chain — anyone
> can run a batch-anchor instance, and re-submitting an already-anchored CID is a
> safe no-op.

---

## Build

```sh
# From this directory (it is its own workspace):
cargo build --release
```

Produces `target/release/wb-batch-anchor-lez`.

---

## Usage

Same subcommands as `wb-batch-anchor`, but real on-chain:

```sh
export WB_SEQUENCER_URL=http://localhost:3040
export WB_PROGRAM_ID=<deployed program id>
export WB_SIGNER_KEY=<hex signing key>

# Daemon: subscribe to the Delivery topic, accumulate (CID, metadata_hash)
# tuples, and anchor them on-chain in idempotent, resumable batches.
wb-batch-anchor-lez run --batch-size 50

# One-shot: drain the topic for a few rounds, anchor what accumulated, exit.
wb-batch-anchor-lez anchor

# Query the on-chain registry for a CID.
wb-batch-anchor-lez query <cid>
wb-batch-anchor-lez query <cid> --json
```

`publish` and `status` also work (they don't touch the registry: `publish` uses
Storage + Delivery, `status` reads the local checkpoint).

---

## How `get_by_cid` implements "queryable by CID"

The on-chain program stores each document's `RegistryRecord` in a **program-
derived account (PDA)** whose address is deterministically derived from the CID —
no on-chain index or scan is needed. `LezRegistry::get_by_cid` reproduces that
derivation and reads the account directly:

1. Compute the seed: `cid_seed(cid) = SHA256(b"WB-CID-PDA-v1" || cid)`
   (from `wb_registry_core`, shared with the program so host and guest agree).
2. Derive the account id:
   `AccountId::for_public_pda(&program_id, &PdaSeed::new(cid_seed(cid)))`.
3. `client.get_account(pda)`: if the account is present with non-empty data,
   borsh-decode `wb_registry_core::RegistryRecord` and convert it to
   `wb_types::RegistryRecord`; otherwise the CID isn't anchored, so return
   `Ok(None)`.

Because the address is a pure function of the CID, a lookup is a single
point-read — O(1), no enumeration — which is what satisfies the prize's
"queryable by CID" requirement.

### A note on `anchor_batch` receipts

`anchor_batch` returns every submitted CID under `AnchorReceipt.anchored` and
leaves `already_present` empty. On-chain idempotency makes re-submits safe, but
the sequencer's submit response doesn't itself say which CIDs were *newly*
written vs already present; computing that split precisely requires reading each
PDA back (via `get_by_cid`) or having the program emit per-entry events. We skip
that round-trip on the hot path — downstream logic treats both buckets as
"anchored".

---

## Status of the LEZ v0.1.2 bindings

`src/lib.rs` is written against the verified LEZ v0.1.2 client facts, but every
call whose exact signature could not be confirmed offline is marked with
`// TODO(verify against LEZ v0.1.2):`. Grep for them before the first compile:

```sh
grep -rn "TODO(verify against LEZ v0.1.2)" src/
```

Prefer wiring the **SPEL-generated typed client** for instruction encoding; the
hand-built `Message` path is the fallback.
