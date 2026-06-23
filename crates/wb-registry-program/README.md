# whistleblower_registry — on-chain CID registry (LEZ program)

The on-chain anchor for Whistleblower (LP-0017): a [SPEL](https://github.com/logos-co/spel)
`#[lez_program]` for the Logos Execution Zone that records `(CID, metadata_hash,
anchor_timestamp)` per document, accepts batches, is idempotent, and is queryable
by CID.

## Why a LEZ program (not the zone SDK)

LP-0017 lets the submitter choose a **LEZ program** or **direct consensus
inscription via the zone SDK**, with justification. We chose the **LEZ program**:

1. **Permissionlessness.** The batch-anchor tool must be runnable by *anyone*
   with no coordination. Decentralised sequencers for zones are not yet shipped,
   so the zone-SDK path requires a single designated actor to perform consensus
   inscription — a central point that breaks the censorship-resistance and
   permissionless guarantees at the heart of this prize.
2. **Verifiable, self-contained rules.** A program encodes the registry's rules
   (PDA-per-CID, idempotency, batch validation) as code anyone can audit and
   invoke, rather than relying on an off-chain actor's discipline.
3. **Tooling fit.** LP-0017's IDL requirement (SPEL) applies to the LEZ-program
   path, giving us generated clients for both the CLI adapter and the GUI.

Trade-off: real proof generation (`RISC0_DEV_MODE=0`) is computationally heavy.
That is acceptable here — anchoring is occasional and batched.

## Design

* **One PDA per CID.** Each document is stored in a program-derived account whose
  id is deterministic in the CID:
  `pda = for_public_pda(self_program_id, PdaSeed::new(cid_seed(cid)))`,
  with `cid_seed(cid) = SHA256("WB-CID-PDA-v1" || cid)` (CIDs exceed the 32-byte
  seed limit, so they are hashed). Derivation lives in `wb_registry_core` so the
  guest and the off-chain query path agree exactly.
* **Account data** = `borsh(RegistryRecord { cid, metadata_hash, anchor_timestamp })`.
* **`anchor_batch(entries, anchor_timestamp, records)`** registers many CIDs in
  one transaction; `records[i]` is the PDA for `entries[i].cid`. The batch tool
  submits ≥ 10; 50 is the benchmark target.
* **Idempotent.** A PDA that is already initialized is passed through unchanged
  (claim-if-default), so re-anchoring a known CID — or two anchorers racing on
  the same CID — never fails.
* **Query by CID** is off-chain: derive the PDA, read the account, borsh-decode
  `RegistryRecord` (implemented in `crates/wb-lez-registry`).
* **No on-chain events.** The LEZ v0.1.2 / SPEL stack has no event mechanism
  (the LP-0012 `emit_event` API does not exist at this version), so the registry
  exposes state via account reads only.

The submitter-supplied `anchor_timestamp` is attested by whoever anchors, not by
consensus (LEZ v0.1.2 exposes no deterministic on-chain clock to the guest). The
integrity-critical binding (CID ↔ metadata_hash) is content-addressed and
tamper-evident regardless.

## Layout

```
wb_registry_core/        shared types + cid_seed (compiles for guest and host)
methods/                 risc0-build harness (embeds the guest ELF + image id)
methods/guest/           the program (#[lez_program]) — detached riscv32 workspace
examples/                generate_idl + the SPEL CLI
spel.toml, Makefile      build/IDL/deploy/codegen entrypoints
```

## Build, IDL, deploy

Prerequisites: the RISC0 toolchain (`curl -L https://risczero.com/install | bash
&& rzup install`) and the **LEZ #468 `ring` patch** (see `methods/guest/Cargo.toml`
— fork LEZ with `risc0-zkvm` default-features disabled and uncomment the
`[patch]`). Then:

```bash
# Compile the guest program (reproducible, needs Docker)…
make build
# …or without Docker, using the installed RISC0 rust toolchain:
make build-local

# Generate the IDL (required deliverable):
make idl                      # -> whistleblower_registry-idl.json

# Deploy to a LEZ devnet/testnet or a local sequencer; prints the program id
# (the program "address" IS the RISC0 image id, derived from the ELF):
make deploy

# Generate clients from the IDL:
make ffi-gen                  # Rust+FFI client for crates/wb-lez-registry
make ui-gen                   # Qt/QML module for app/ (whistleblower_registry)
```

## Compute-unit / cycle benchmarks

LEZ has no "compute unit"; cost is **RISC0 zkVM cycles**. Measure a single-CID
anchor (`entries.len() == 1`) and a 50-CID batch (`entries.len() == 50`) — see
`docs/benchmarks.md`. Run measurements with `RISC0_DEV_MODE=0` for real proving.

## Build status (verified)

Compiled against the real SPEL + LEZ v0.1.2 stack on an Apple M4 (RISC0 toolchain
1.94.1, `ring` patch applied per `methods/guest/Cargo.toml`):

- `make idl` → `whistleblower_registry-idl.json` (committed) ✓
- `cargo build -p wb-registry-methods --release` cross-compiles the guest to
  `riscv32im-risc0-zkvm-elf` ✓
- **Program image id (the on-chain "address", a deterministic function of the ELF):**

  ```
  WB_REGISTRY_ID: [u32; 8] = [736378292, 3237769127, 3218962078, 1003268346,
                              3355061132, 654317770, 4171522436, 2002532608]
  ```

  (image id as of the `anchor_one` addition; it changes on every guest-source change)

Implementation note settled during the build: the SPEL `#[lez_program]` macro
rewrites a plain `SpelOutput::execute(..)` call by reading a private field on its
elements, so the program emits its post-states via `SpelOutput::execute_with_claims`
directly (claim-if-default per account for idempotency). See the guest source.

A real `anchor_one` + `inspect` round-trip succeeded on a standalone LEZ
sequencer with `RISC0_DEV_MODE=0` (real proving). `anchor_one` is a scalar-arg
sibling of `anchor_batch` (same per-CID PDA + idempotent claim-if-default logic)
so the IDL-driven SPEL CLI — which cannot encode `Vec<struct>` — can submit it:

- Deployed `wb_registry.bin` (image id above) via `wallet deploy-program` ✓
- Derived the per-CID PDA for `zDvTestWhistleblowerCID0001`:
  `76d5eUdWWRBf7SbcSQFHEmzD4bJKAjqbK3mAqwecR1Tg` ✓
- `wb_registry_cli anchor-one --cid … --metadata-hash … --anchor-timestamp …
  --record <PDA>` submitted + confirmed (tx
  `32673a9feb866e56f3fb0ac7ad8136804cd82fd07f712953bc4c0318c0c17213`) ✓
- `wb_registry_cli inspect <PDA> --type RegistryRecord` decoded the stored
  `RegistryRecord` back (cid + metadata_hash + anchor_timestamp) — proving
  query-by-CID ✓

Remaining: the real-proof demo and CU benchmarks (`RISC0_DEV_MODE=0`). See
`../../HANDOFF.md`.
