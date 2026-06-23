<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# GitHub issues to file (LP-0017 requires reporting Logos-tech problems)

Ready-to-paste issues for problems hit while building Whistleblower. Each lists
the target repo, a title, and a body. (#5 already exists upstream — reference it.)

---

## 1. `logos-co/spel` — SPEL CLI/codegen can't encode `Vec<struct>` instruction args

**Title:** IDL CLI cannot serialize a `Vec<#[account_type] struct>` instruction argument

**Body:**
The generated IDL-driven CLI (`spel-cli` `parse.rs`/`serialize.rs`) has no case for
`Vec<Defined>`: a `Vec` of a struct argument falls through to `ParsedValue::Raw`,
and `to_dynamic_value` has no `(Vec, Raw)` handler for it, so submission fails with:
`type mismatch: expected Vec { vec: Defined { defined: "AnchorArg" } }, got Raw(...)`.

Repro: declare an instruction `fn anchor_batch(.., entries: Vec<AnchorArg>, ..)` where
`AnchorArg` is an `#[account_type]` struct; `make idl`; then
`<prog>_cli ... anchor-batch --entries '[{...}]'`.

Impact: batch instructions that take a `Vec` of structs cannot be driven by the
generated CLI. Workaround: submit batches via a custom client; expose a scalar
sibling instruction for the single-entry path.

## 2. `logos-co/spel` — README API drift (`states_only`, `spel_cli::run`)

**Title:** README references non-existent `SpelOutput::states_only` and `spel_cli::run()`

**Body:** The README shows `SpelOutput::states_only(...)` — no such method exists
(the real API is `SpelOutput::execute(...)` / `execute_with_claims(...)`), and
`spel_cli::run()` (the crate is `spel`, so it's `spel::run()`). Copy-pasting from
the README does not compile. Please update the docs.

## 3. `logos-storage/logos-storage-nim` — upload Content-Type + base-path mismatch

**Title:** `/data` rejects default curl Content-Type; node base path `/api/storage/v1` differs from published SDKs

**Body:** Two papercuts:
1. `POST {base}/data` without `Content-Type: application/octet-stream` (curl's default
   `application/x-www-form-urlencoded`) returns `The MIME type
   'application/x-www-form-urlencoded' is not valid`. A clearer error or accepting the
   default would help.
2. A current node serves `/api/storage/v1`, but the published JS SDK
   (`@codex-storage/sdk-js`) and Python client default to `/api/codex/v1`, so they
   mismatch a current node. Please align the SDKs (or document the split).

## 4. `logos-blockchain/logos-execution-zone` — no public LEZ sequencer endpoint to deploy to

**Title:** No public LEZ sequencer endpoint for program deployment (only a mock-settlement standalone sequencer)

**Body:** Prizes ask submitters to deploy to "LEZ devnet/testnet with a documented
program address," but there is no published public **LEZ sequencer** RPC. The
public testnet that *is* documented (the `logos-blockchain` node: cryptarchia
consensus + wallet + faucet on `:8080`) is the **L1 / settlement layer**
("Bedrock"/"Mantle"), not a LEZ execution endpoint — a LEZ program cannot be
deployed to it directly.

From the LEZ source (v0.1.2): the sequencer settles each block to Bedrock via
`submit_inscribe_tx_to_bedrock(SignedMantleTx)`, and the only runnable local mode,
`--features standalone` (= `sequencer_core/mock`), **mocks** the Bedrock + Indexer
clients. So a self-hosted public LEZ testnet means running THREE services — an L1
(Bedrock/cryptarchia) node (whose bootstrap the node docs say takes 12–24h), an
Indexer, and a **non-standalone** sequencer wired to both — with no published
endpoints or end-to-end guide.

Ask: publish a public LEZ devnet sequencer RPC (+ the deploy flow against it), or a
one-command non-standalone bring-up (L1 + indexer + sequencer), or clarify that a
standalone-sequencer deployment with a documented image id satisfies the prize
requirement. (For LP-0017 we deployed to a local standalone sequencer and document
the image id as the program address; anchors run with `RISC0_DEV_MODE=0`.)

## 5. `logos-blockchain/logos-execution-zone` — guest cross-compile pulls `ring` (riscv32) [#468]

**Title:** (reference existing issue #468) `ring` fails to cross-compile for `riscv32im-risc0-zkvm-elf`

**Body:** Building any SPEL/LEZ guest pulls `risc0-zkvm` default features →
`reqwest → rustls → ring`, which can't cross-compile to riscv32. Workaround used:
fork LEZ, set `risc0-zkvm = { default-features = false, features = ["std"] }`, and
`[patch]` `nssa`/`nssa_core` to the fork. Filing/【referencing】 so a default-features
fix or a doc note lands upstream.

## 6. `logos-messaging/logos-delivery` — no prebuilt macOS-arm64 node binary

**Title:** Publish prebuilt macOS-arm64 `wakunode2`/`logos-delivery` binaries

**Body:** Releases provide Linux binaries but no `darwin-arm64` build, and there is no
macOS Waku binary upstream. On a Mac without Docker/Nim there is no way to run a
Delivery node for local development. `logos-storage-nim` already ships
`logos-storage-darwin-arm64` — please do the same for Delivery.
