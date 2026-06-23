<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Whistleblower — Benchmarks (RISC0 cycles, not "compute units")

LP-0017 asks for a cost characterization of anchoring. A note up front, because
it changes how the numbers are read:

> **LEZ has no "compute unit" (CU) concept.** The Logos Execution Zone executes
> programs as **RISC0 zkVM guests**, and the cost that matters is **RISC0
> cycles** — the number of zkVM instructions executed — together with the
> **wall-clock time to generate the zero-knowledge proof** over those cycles.
> There is no per-instruction "gas"/"CU" meter to report. So wherever a Solana-
> style submission would quote "compute units consumed", we instead report
> **user cycles**, **total cycles**, **prove time**, and **proof size**.

We benchmark the `whistleblower_registry` program's `anchor_batch` instruction in
two shapes:

- **single-CID** — `anchor_batch` with a batch of length **1** (one PDA written),
- **50-CID** — `anchor_batch` with a batch of length **50** (the runner's default
  `batch_size`, and the LP-0017 batch target).

---

## Cycles vs. proof time — what each column means

| Term | What it is |
| --- | --- |
| **user cycles** | Cycles executed by the guest *program logic* (`anchor_batch`: borsh-decode the entries, derive/validate each PDA, borsh-encode each `RegistryRecord`, build the post-states). Scales with batch length. |
| **total cycles** | User cycles **plus** zkVM overhead (the cycle count actually proven; padded to a power of two per RISC0 segmenting). This is the figure that drives prove time. |
| **prove time** | Wall-clock time for the prover to produce the proof over `total_cycles`. Hardware- and backend-dependent (CPU vs Metal vs CUDA). |
| **proof bytes** | Serialized size of the resulting receipt/proof. |

`anchor_batch` is *linear* in batch length: a 50-CID batch does ~50× the
per-entry work of a single-CID batch (50 SHA-256 PDA derivations + 50 borsh
record encodes + 50 account writes), so its user-cycle count should be roughly
50× the single-CID user cycles **plus** the fixed per-invocation overhead. Total
cycles (and thus prove time) grow with that, but step up in chunks because RISC0
pads each proof segment to a power-of-two cycle count.

Batching is the whole point: one proof amortized over 50 CIDs is dramatically
cheaper *per CID* than 50 separate single-CID proofs, which is why the runner
accumulates tuples before anchoring.

---

## How to measure

All measurements must be taken with **real proofs**:

```sh
export RISC0_DEV_MODE=0
```

With `RISC0_DEV_MODE=1` the prover is skipped (dev/fast mode) — cycle counts may
still be reported by the executor, but **prove time and proof bytes are
meaningless**, so they cannot be used for the LP-0017 numbers or the narrated
video.

### Option A — cycle counts from the executor (fast, no full proof)

RISC0's executor reports the session's cycle totals without generating a proof.
Inside the program crate (`crates/wb-registry-program`), a small bench harness
that builds the same `anchor_batch` input the host sends and runs the guest under
the executor will print `user_cycles` and `total_cycles` per batch size. Run it
for batch length 1 and 50:

```sh
cd crates/wb-registry-program
# tools/cycle_bench is the intended home for this harness (executor-only; prints
# user_cycles / total_cycles for batch sizes 1 and 50). It does NOT need a
# sequencer. See HANDOFF.md for wiring it on the build machine.
cargo run --release --bin cycle_bench -- --batch 1
cargo run --release --bin cycle_bench -- --batch 50
```

> The executor path gives you the **cycle** columns quickly. It does not give a
> trustworthy prove time / proof size — for those, use Option B.

### Option B — end-to-end prove time (real proof, real numbers)

Drive a real `anchor_batch` through the standalone sequencer with
`RISC0_DEV_MODE=0` and time it:

```sh
export RISC0_DEV_MODE=0
# Sequencer running (scripts/run-sequencer.sh) and program deployed.

# single-CID: publish one doc, then anchor (batch length 1)
time wb-batch-anchor-lez anchor          # after a single publish

# 50-CID: publish 50 docs (or one publish loop), then anchor a full batch
time wb-batch-anchor-lez anchor --rounds 5
```

The sequencer's `RUST_LOG=info` output prints proof generation progress; capture
the prove-time and the proof/receipt size it reports. The narrated video should
show this terminal output, including the proof being generated.

---

## Results table (template)

Fill in from your runs on the provisioned machine. Record the exact hardware and
prover backend used (it dominates prove time).

> **Apple Silicon caveat.** Apple Silicon (e.g. M2 Pro / M4) has **no CUDA GPU**.
> RISC0 falls back to CPU or the Metal backend, so real proofs are **minutes-
> scale**, not seconds. LEZ's own published benchmarks (16 GB Apple M2 Pro,
> CPU/Metal, no CUDA) put a **single real private proof at ~13–60 s** and
> **public execution at tens of milliseconds**. Treat the reference row below as
> an order-of-magnitude expectation for Apple Silicon; a CUDA host would be much
> faster.

| Scenario | public-exec time (measured) | user_cycles | total_cycles | prove_time | proof_bytes |
| --- | --- | --- | --- | --- | --- |
| single-CID (`anchor_one`, 1 PDA) | **~3–7 ms** (M4, `RISC0_DEV_MODE=0`) | _pending cycle_bench_ | _pending_ | n/a (public exec) | n/a (public exec) |
| 50-CID (`anchor_batch`, 50 PDAs) | _pending batch encoder_ | _pending_ | _pending_ | n/a (public exec) | n/a (public exec) |

**Measured on the build mini** (Mac mini M4, 16 GB, macOS 15.3.2, RISC0 3.0.5,
`RISC0_DEV_MODE=0`): a real single-CID anchor executes in **~3–7 ms** on the
standalone sequencer's executor.

> **Key nuance:** the registry uses **public** accounts, so anchoring runs as
> **public execution** on the sequencer's executor — there is **no per-transaction
> STARK proof** for a public anchor, which is why it's milliseconds, not the
> minutes-scale of a *private* proof. `RISC0_DEV_MODE=0` is still in force (real
> executor + real block-level receipts). The `prove_time`/`proof_bytes` columns
> apply only to private-proof workloads, hence "n/a (public exec)" here.

Still pending: (1) isolated per-op **user/total cycle counts** via an executor
`cycle_bench` harness (the sequencer log reports wall-time, not isolated cycles);
(2) the **50-CID** row, which needs the batch instruction encoded via the
SPEL-generated client (`make ffi-gen`) — see SUBMISSION.md F4 and the
`wb-lez-registry` adapter note.

**Reference numbers (LEZ benchmarks, 16 GB Apple M2 Pro, CPU/Metal, no CUDA):**

| Quantity | Reference value | Source |
| --- | --- | --- |
| Single real **private** proof | **~13–60 s** | LEZ benchmarks (Apple M2 Pro, no CUDA) |
| Public execution (no proof) | **~tens of ms** | LEZ benchmarks (Apple M2 Pro, no CUDA) |
| GPU acceleration on Apple Silicon | **none (no CUDA)** → proofs are minutes-scale under load | Apple Silicon hardware reality |

Record alongside the table:

- **Host:** CPU/GPU model, RAM, OS (e.g. "Mac mini M4, 16 GB, macOS").
- **Prover backend:** CPU / Metal / (CUDA if applicable).
- **RISC0 version:** `=3.0.5` (pinned by the program).
- **`RISC0_DEV_MODE`:** must be `0` for any reported prove_time / proof_bytes.

---

## See also

- `docs/ARCHITECTURE.md` — the `anchor_batch` instruction and PDA scheme that
  determine the cycle profile.
- `HANDOFF.md` — installing RISC0, building the program, running the sequencer
  with `RISC0_DEV_MODE=0`.
