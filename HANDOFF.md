<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Whistleblower — Handoff: finishing on a provisioned machine

This is the exact, ordered procedure to take Whistleblower (LP-0017) from "Rust
core builds & tests anywhere" to a **real, on-chain, proof-generating** end-to-end
demo on a provisioned machine (target: a Mac mini M4; the steps are
OS-agnostic). The host-only Rust workspace already builds and passes
`cargo test --workspace` (27 unit tests + the `wb-e2e` pipeline test); what
remains is the RISC0/SPEL/LEZ build graph, the Nix/Qt app, and the recorded demo.

Why this is a separate handoff: the authoring Mac has **no RISC0, Nix, or Qt**,
and LEZ pins pre-1.0 git deps that need a `ring` patch (issue #468) to
cross-compile a riscv32 zkVM guest. All of that happens here.

> Conventions below assume the repo root is the current directory unless a `cd`
> says otherwise. Make the scripts executable once: `chmod +x scripts/*.sh`.

---

## 0. Prerequisites sanity check

```sh
# Rust 1.94 (pinned by rust-toolchain.toml). Confirm the host workspace is green:
cargo test --workspace        # expect 27 unit tests + wb-e2e to pass

# Tools you will need on this machine (install per their own docs):
#   - docker            (for the storage/delivery nodes, optional if using native binaries)
#   - Nix (flakes) + Qt6 (for the Basecamp app)
#   - the LEZ `wallet`, `spel-client-gen`, and `cargo risczero` / `cargo-risczero`
#     CLIs (from the RISC0 + LEZ/SPEL toolchains installed below)
```

---

## 1. Install the RISC0 toolchain

```sh
curl -L https://risczero.com/install | bash
rzup install
# Ensure the RISC0 bin dir is on PATH (the installer prints the location, e.g.):
export PATH="$HOME/.risc0/bin:$PATH"
```

This provides the riscv32 zkVM guest compiler and `cargo risczero`.

---

## 2. Fork + patch LEZ for issue #468 (`risc0`/`ring` cross-compile)

LEZ enables `risc0-zkvm`'s default features, which pull `ring`, which does **not**
cross-compile to `riscv32im-risc0-zkvm-elf`. You must disable those defaults in a
LEZ fork and `[patch]` the build to use it.

```sh
# Clone LEZ next to the repo (the shipped patch paths assume ./vendor/…):
git clone https://github.com/logos-blockchain/logos-execution-zone \
  vendor/logos-execution-zone
git -C vendor/logos-execution-zone checkout v0.1.2
```

In that checkout, set **every** `risc0-zkvm` dependency to disable default
features (re-enabling only `std` and whatever the host genuinely needs):

```toml
# in each LEZ Cargo.toml that depends on risc0-zkvm:
risc0-zkvm = { version = "3.0.5", default-features = false, features = ["std"] }
```

Then wire the patch in **two** places — the guest (which actually
cross-compiles) and the host adapter:

**(a) Guest** — `crates/wb-registry-program/methods/guest/Cargo.toml`. A
commented patch block ships there; uncomment it and point it at your fork/branch:

```toml
[patch."https://github.com/logos-blockchain/logos-execution-zone.git"]
nssa_core = { git = "https://github.com/YOUR-USER/logos-execution-zone.git", branch = "fix-risc0-defaults" }
nssa      = { git = "https://github.com/YOUR-USER/logos-execution-zone.git", branch = "fix-risc0-defaults" }
```

(Equivalently, point the patch at the local `vendor/logos-execution-zone`
checkout via `path = …` instead of `git = …`.)

**(b) Host adapter** — `crates/wb-lez-registry/Cargo.toml`. A commented patch
block ships there too; uncomment and adjust paths to your checkout:

```toml
[patch."https://github.com/logos-blockchain/logos-execution-zone.git"]
nssa_core             = { path = "../../vendor/logos-execution-zone/nssa_core" }
nssa                  = { path = "../../vendor/logos-execution-zone/nssa" }
common                = { path = "../../vendor/logos-execution-zone/common" }
sequencer_service_rpc = { path = "../../vendor/logos-execution-zone/sequencer_service_rpc" }
```

> The key directive in all cases is `risc0-zkvm` `default-features = false`. For a
> host-only build the `ring` break may not trigger, but having the patch ready
> means this machine only has to flip it on.

---

## 3. Build the registry program (RISC0 + SPEL)

The program is `crates/wb-registry-program` (its own workspace; the guest under
`methods/guest` is a *further* detached workspace). Two build paths:

```sh
cd crates/wb-registry-program

# Reproducible guest build (requires Docker):
make build
#   -> methods/guest/target/riscv32im-risc0-zkvm-elf/docker/wb_registry.bin

# OR the no-Docker path using the installed RISC0 rust toolchain:
make build-local
#   -> methods/guest/target/riscv32im-risc0-zkvm-elf/release/wb_registry
```

`spel.toml` and the `Makefile` default to the **Docker** binary path
(`…/docker/wb_registry.bin`); if you used `make build-local`, set
`PROGRAM_BIN=methods/guest/target/riscv32im-risc0-zkvm-elf/release/wb_registry`
when you run `make deploy` (or edit `spel.toml`).

The host members (`wb_registry_core`, `methods`, `examples`) build natively.

---

## 4. Generate the IDL and the clients

```sh
cd crates/wb-registry-program

# SPEL IDL JSON (default output: whistleblower_registry-idl.json):
make idl              # runs: cargo run --bin generate_idl > whistleblower_registry-idl.json

# Rust+FFI client consumed by the on-chain CLI adapter (wb-lez-registry):
make ffi-gen          # spel-client-gen --idl <idl> --out-dir generated/rust-ffi --target rust+ffi

# The Qt/QML Basecamp module consumed by the app:
make ui-gen           # spel-client-gen --idl <idl> --out-dir ../../app/generated \
                      #   --target logos-module --module-name whistleblower_registry
```

> **Prefer the SPEL-generated typed client for instruction encoding.** The
> hand-written `encode_instruction` in `crates/wb-lez-registry/src/lib.rs` is a
> *fallback* that assumes the program's dispatch wants a bare borsh tuple of the
> args with **no selector prefix**. The generated client knows the exact
> instruction discriminant + argument layout. After `make ffi-gen`, depend on the
> generated crate and swap `encode_instruction` (ideally the whole `Message`
> build) for the generated `anchor_batch(...)` builder. This is the single most
> important thing to verify before trusting on-chain writes.
>
> Also grep the adapter for the unverified-API markers and resolve each against
> the installed LEZ:
> ```sh
> grep -rn "TODO(verify against LEZ v0.1.2)" crates/wb-lez-registry/src/
> ```
> In particular: the `nssa_core` / `common` import paths, `SequencerClientBuilder`,
> `AccountId::for_public_pda`, `get_accounts_nonces`, `Message::try_new`,
> `WitnessSet::for_message`, `PublicTransaction::new` / `LeeTransaction::Public`,
> `send_transaction`'s return type, `get_account`'s return shape (implement the
> `MaybeAccount` trait for it), and `parse_signer_key` (wire the real signing-key
> constructor — it currently fails loudly on purpose).

---

## 5. Run a standalone LEZ sequencer

```sh
# Helper (point LEZ_DIR at your checkout from step 2; forces RISC0_DEV_MODE=0):
LEZ_DIR=vendor/logos-execution-zone scripts/run-sequencer.sh
```

which runs the documented invocation:

```sh
RUST_LOG=info RISC0_DEV_MODE=0 \
  cargo run --features standalone -p sequencer_service \
  <path/to/configs/debug/sequencer_config.json>
```

The sequencer exposes JSON-RPC on **:3040** (the `wb-batch-anchor-lez` default
`WB_SEQUENCER_URL`). Leave it running in its own terminal. With
`RISC0_DEV_MODE=0` it generates **real** RISC0 proofs for `anchor_batch`.

---

## 6. Deploy the program and record the program id

```sh
cd crates/wb-registry-program
make deploy            # runs: wallet deploy-program <PROGRAM_BIN>
#   -> prints the program id, which IS the RISC0 image id of the guest.
```

Record that program id — it is what queries derive PDAs against:

```sh
export WB_PROGRAM_ID=<the printed program id>     # 64 hex chars, or 8 comma-separated u32 words
export WB_SIGNER_KEY=<hex signing key>            # authorizes/pays the anchor tx (anchoring is permissionless)
```

Put the program id (and the sequencer/storage/delivery endpoints) into the
**Deployment addresses** table in `README.md`.

---

## 7. Run the Storage (Codex) and Delivery (Waku) nodes

```sh
# Best-effort docker/binary launcher (adjust image names/versions inside it):
scripts/run-nodes.sh
```

It exposes:

- **Logos Storage (Codex)** REST on **:8080** — base path `/api/storage/v1` on
  current nodes (some SDKs use `/api/codex/v1`; point `WB_STORAGE_URL` at whatever
  your node serves).
- **Logos Delivery (Waku/nwaku)** REST on **:8645** (relay autosharding API).

```sh
export WB_STORAGE_URL=http://localhost:8080/api/storage/v1
export WB_DELIVERY_URL=http://127.0.0.1:8645
```

---

## 8. Build the on-chain CLI and run the real demo

```sh
# Build the on-chain binary (its own workspace; needs the patches from step 2):
cd crates/wb-lez-registry && cargo build --release && cd ../..
#   -> crates/wb-lez-registry/target/release/wb-batch-anchor-lez

# Build the local CLI used for `publish`:
cargo build --release -p wb-batch-anchor

# With nodes + sequencer running and WB_PROGRAM_ID / WB_SIGNER_KEY exported:
scripts/demo.sh
```

`scripts/demo.sh` forces `RISC0_DEV_MODE=0` and runs:

1. `wb-batch-anchor publish scripts/sample-doc.txt …` — upload to Storage (CID) +
   broadcast the envelope over Delivery.
2. `wb-batch-anchor-lez anchor` — drain the topic, pick up the CID, and anchor it
   on-chain. **This is where the real RISC0 proof is generated.**
3. `wb-batch-anchor-lez query <cid>` — confirm the CID is registered.

---

## 9. Record the narrated video (real proofs)

Record the demo with `RISC0_DEV_MODE=0` so the terminal shows **proof
generation**:

- Show `echo $RISC0_DEV_MODE` (must be `0`).
- Run `scripts/demo.sh` (or the steps manually) and let the camera capture the
  sequencer's `RUST_LOG=info` proof-generation output during step 2 — on Apple
  Silicon (no CUDA GPU) this takes seconds-to-minutes, which is the point.
- Show the final `query <cid>` returning the `RegistryRecord`.
- Optionally show the Basecamp app (step 10) doing publish→broadcast→anchor.

---

## 10. (App) Build and load the Basecamp module

```sh
cd app
nix build            # the ui_qml module (Nix flakes + Qt6 required)
nix build .#lgx      # the packaged .lgx -> ./result
```

Align `app/flake.nix` / `app/CMakeLists.txt` with the installed
`logos-module-builder` API if the build complains (the template entry points vary
by version — see the comments in those files). Install the dependency modules
(`storage_module`, `delivery_module`, and the **generated** `whistleblower_registry`
from step 4's `make ui-gen`) into Logos Basecamp, then load the app and run
publish → broadcast → anchor. See `app/README.md`.

---

## 11. Benchmarks

Fill in `docs/benchmarks.md` from this hardware, with `RISC0_DEV_MODE=0`:

- single-CID (`anchor_batch` batch len 1) and 50-CID (batch len 50);
- record `user_cycles`, `total_cycles`, `prove_time`, `proof_bytes`, plus the
  host CPU/GPU, RAM, OS, and prover backend (CPU/Metal — Apple Silicon has no
  CUDA, so expect minutes-scale proofs).

---

## LP-0017 success-criteria checklist

Map **every** criterion to where it is satisfied or what remains. ✅ = done and
exercised on the host already; ⚙️ = source/tooling complete, finish on this
machine via the step noted.

| # | Criterion | Where satisfied | Status / remaining |
| --- | --- | --- | --- |
| 1 | Upload to Logos Storage → CID | `wb-index` `HttpStorage` + `Publisher::upload`; app `storage_module.uploadUrl` | ✅ tested; ⚙️ live upload via §7 node |
| 2 | Retry transient Storage failures (back-off) | `wb-index` `RetryPolicy` | ✅ tested |
| 3 | Broadcast metadata envelope over Logos Delivery | `wb-index` `HttpDelivery` + `Publisher::broadcast`; app `delivery_module.send` | ✅ tested; ⚙️ live broadcast via §7 node |
| 4 | Delivery topic `/whistleblower/1/documents/json` | `wb-types::topic`; app `Main.qml`; CLI default | ✅ tested |
| 5 | Minimum envelope fields | `wb-types` `MetadataEnvelope` | ✅ tested |
| 6 | Canonical, cross-language `metadata_hash` | `wb-types` `metadata_hash` + C++/QML `WhistleblowerBackend`; shared vector | ✅ tested both sides |
| 7 | Permissionless on-chain anchoring | `wb-registry-program` `anchor_batch`; `wb-lez-registry` `LezRegistry`; `wb-batch-anchor-lez` | ⚙️ build §2–4, run §5–8 |
| 8 | One PDA per CID, deterministic from CID | `wb_registry_core::cid_seed`; program; `LezRegistry::pda_for_cid` | ✅ `cid_seed` tested; ⚙️ on-chain via §6/§8 |
| 9 | Idempotent anchoring | program `new_claimed_if_default`; `Mock`/`FileRegistry`; runner dedup | ✅ tested; ⚙️ confirm on-chain in §8 |
| 10 | Batch anchoring (≥ 10; 50 target) | `BatchAnchorRunner` (`batch_size = 50`); `anchor_batch(Vec<…>)` | ✅ tested |
| 11 | Crash-safe resume | `Checkpoint` + `CheckpointStore` | ✅ tested |
| 12 | Queryable by CID | `RegistryClient::get_by_cid`; `LezRegistry` PDA point-read; CLI `query` | ✅ file-backed tested; ⚙️ on-chain via §8 |
| 13 | SPEL IDL + generated clients | `make idl` / `make ffi-gen` / `make ui-gen` | ⚙️ §4 |
| 14 | Generated typed client used for instruction encoding | `make ffi-gen` then swap `encode_instruction` | ⚙️ §4 (prefer over fallback) |
| 15 | Basecamp app (upload + broadcast + anchor) | `app/` (`ui_qml`) | ⚙️ §10 (Nix/Qt) |
| 16 | Real proofs (`RISC0_DEV_MODE=0`) in the demo | `scripts/run-sequencer.sh`, `scripts/demo.sh` | ⚙️ §5, §8 |
| 17 | Narrated video showing proof generation | this doc §9 | ⚙️ §9 |
| 18 | Cost characterization (RISC0 cycles, not "CU"): single-CID vs 50-CID | `docs/benchmarks.md` | ⚙️ §11 |
| 19 | Deployment addresses recorded | `README.md` "Deployment addresses" | ⚙️ §6 |
| 20 | Dual license MIT OR Apache-2.0 | `LICENSE-MIT`, `LICENSE-APACHE`; per-crate `license` | ✅ done |

### Known upstream issues to keep tracking (also in README)

- **LEZ #468** — `risc0`/`ring` riscv32 cross-compile break (the §2 patch).
- **Storage base-path split** — `/api/storage/v1` (node) vs `/api/codex/v1` (SDKs).
- **No documented public LEZ testnet RPC** — hence the standalone sequencer (§5).
- **SPEL README staleness** — `SpelOutput::states_only` doesn't exist (use
  `execute`, which the program does); `spel_cli::run()` should be `spel::run()`.
