# Whistleblower (LP-0017) — Narrated Demo Video Script

Target length: **8–10 minutes**. Format: screen recording of a terminal (and
briefly the README/architecture diagram), with voice-over. LP-0017 requires a
*narrated* walkthrough — explain **what** you built, **why**, the **architecture
and key decisions**, and demonstrate the **end-to-end flow**, with terminal
output that visibly confirms **`RISC0_DEV_MODE=0`** (real proving).

The script is two columns: **SAY** (voice-over) and **SHOW** (on screen). Exact
commands and the real values captured on the build mini are in the Appendix.

---

## Pre-flight checklist (do before recording — keep OUT of the video)

- [ ] Terminal font large (≥ 18pt), dark theme, wide window.
- [ ] On the build host (the Mac mini): the standalone LEZ **sequencer is running**
      with `RISC0_DEV_MODE=0`, the **registry program is deployed**, and a
      **Logos Storage node** (`:8080`) and **Logos Delivery node** (`:8645`) are up.
      (See `HANDOFF.md` / `scripts/run-nodes.sh` / `scripts/run-sequencer.sh`.)
- [ ] `echo $RISC0_DEV_MODE` prints `0` in the shell you'll record.
- [ ] A sample document ready (`scripts/sample-doc.txt`).
- [ ] Repo open in an editor on a second workspace for the architecture beat.
- [ ] Do one dry run end-to-end so the live take is clean.

---

## LIVE DRIVER RUN-LIST (agent runs these on your cue; you narrate)

Cue protocol: you narrate; say **"next"** (or the scene number) and the agent runs
that block. Outputs stay on screen. `‹…›` = values filled from the live run.

| Cue | Agent runs | What viewers see |
|-----|-----------|------------------|
| **Scene 3** | `cargo test --workspace` (laptop) | `… 30 passed` across crates |
| **Scene 4** | `bash scripts/dev-demo.sh` (laptop) | in-process upload→broadcast→anchor→query, incl. dedup + checkpoint/resume |
| **Scene 4 (opt.)** | (mini) real Storage upload: `curl -sS -X POST http://127.0.0.1:8080/api/storage/v1/data --data-binary @scripts/sample-doc.txt` | a real Codex CID `zDv…` from the live Storage node |
| **Scene 5a** | (mini) `echo RISC0_DEV_MODE=$RISC0_DEV_MODE` then `tail -n 30 ~/seq.log` | `0`, and real executor/proving lines |
| **Scene 5b** | (mini) `wb_registry_cli … anchor-one --cid ‹CID› --metadata-hash ‹hex› --anchor-timestamp ‹ms› --record ‹PDA›` | tx accepted; new block in seq.log |
| **Scene 6** | (mini) `wb_registry_cli … inspect ‹PDA› --type RegistryRecord` | decoded `RegistryRecord` |
| **Scene 7** | open `README.md` + `SUBMISSION.md` | criteria coverage |

Notes:
- Scene 4 uses the in-process **dev path** for broadcast (no prebuilt macOS Waku
  binary exists and the Colima VM on this mini has no public-internet egress, so a
  real nwaku node isn't available here — it runs trivially on any Linux box). The
  **Storage upload is real** (live node on the mini) and **Scenes 5–6 are the real
  on-chain anchor+query**; narrate the broadcast honestly as the local dev path.
- The agent executes against the mini over SSH; record the terminal where these
  outputs appear.
- **Rehearse once** (off-camera) right before the take so values/paths are warm
  and the live run is clean.

---

## Scene 1 — What & why  (~0:45)

**SAY:**
> "This is **Whistleblower**, a censorship-resistant document-publishing app built
> on the Logos stack. The problem: whistleblowers and journalists need to publish
> documents that survive takedowns — without a trusted host, without paying
> on-chain fees up front, and with no single point of censorship. Whistleblower
> does it in three moves: **upload** the file to Logos Storage to get a content
> identifier, **broadcast** that CID and its metadata over Logos Delivery so it's
> instantly discoverable, and **anchor** the CID on-chain in a permissionless
> registry for long-term indexing."

**SHOW:** the repo `README.md` title + the ASCII data-flow diagram (file → Storage
[CID] → Delivery [envelope] → batch anchor → LEZ registry).

---

## Scene 2 — Architecture & key decisions  (~1:45)

**SAY:**
> "Four pieces. One: a reusable **document-indexing module**, `wb-index` — the
> upload→broadcast→anchor pipeline, extracted so any Logos app can use it. Two: a
> permissionless **batch-anchor CLI** — anyone can run it to gather broadcast CIDs
> and commit them on-chain in one transaction; no coordination with the original
> publisher. Three: the **on-chain registry**, a SPEL program on the Logos
> Execution Zone. Four: the **Basecamp GUI**.
>
> Key decision: I anchor via a **LEZ program**, not the zone SDK. The tool has to
> be permissionless, and the zone-SDK path currently needs a single designated
> actor to inscribe to consensus — a central choke point that defeats the whole
> purpose. A LEZ program encodes the rules — one account per CID, idempotent
> writes — as code anyone can audit and call.
>
> Two design details I'm proud of: anchoring is **idempotent and resumable** —
> re-anchoring a known CID, or two anchorers racing on the same one, never fails,
> and the batch tool checkpoints its progress so it resumes after a network drop
> without re-processing. And the **metadata hash** is a canonical, language-
> agnostic encoding, so the Rust clients and the C++ GUI produce the identical
> hash."

**SHOW:** scroll `crates/wb-registry-program/README.md` ("Why a LEZ program"),
then `crates/wb-index/README.md` (the trait diagram), then
`crates/wb-types/src/hash.rs` (the canonical hash spec).

---

## Scene 3 — The reusable core, tested  (~0:45)

**SAY:**
> "The core is real code with real tests. Here's the whole workspace: the shared
> types, the indexing module, and the CLI — and the end-to-end pipeline test that
> exercises upload, dedup-broadcast, batch accumulation, idempotent anchoring, and
> checkpoint-resume, all against in-process fakes so it runs anywhere."

**SHOW:**
```
cargo test --workspace
```
Let it print `test result: ok. … 30 passed` across the crates.

---

## Scene 4 — Upload, and instantly findable over Delivery  (~1:30)
*(LP-0017 demo item 1: "a file uploaded and immediately findable via the Logos Delivery topic.")*

**SAY:**
> "Now the live flow. I publish a document. The tool uploads the bytes to Logos
> Storage, gets back a content identifier, builds the metadata envelope —
> title, content-type, size, timestamp, tags — and broadcasts it to the
> Whistleblower Delivery topic. Watch: the moment I publish, a subscriber on that
> topic sees the envelope. It's immediately discoverable, peer-to-peer, no server."

**SHOW:**
```
# (left pane) subscribe to the topic and watch
wb-batch-anchor run --batch-size 10        # the daemon subscribes + waits

# (right pane) publish a document
wb-batch-anchor publish scripts/sample-doc.txt --title "Leaked memo" --tags leak,memo
```
Point at the CID printed by `publish`, then at the daemon pane showing it
**received** the broadcast envelope for that CID.

> Note for recording: this scene needs the Storage (`:8080`) and Delivery
> (`:8645`) nodes running. If a node isn't up, narrate the same flow over
> `scripts/dev-demo.sh` (in-process), and call out that it's the dev path.

---

## Scene 5 — The batch tool anchors it on-chain, with real proofs  (~2:00)
*(LP-0017 demo item 2 + the `RISC0_DEV_MODE=0` requirement.)*

**SAY:**
> "Long-term indexing is decoupled from publishing. Any altruistic third party —
> an NGO, a journalist collective — can run the batch-anchor tool to pick up
> broadcast CIDs and commit them on-chain. First, proof that this is real: the
> sequencer is running with **`RISC0_DEV_MODE=0`** — real RISC Zero proving, not
> dev-mode mocks. Here's the environment, and here's the proving in the log."

**SHOW:**
```
# prove it's real proving, on camera:
echo "RISC0_DEV_MODE=$RISC0_DEV_MODE"          # → 0
tail -f ~/seq.log        # leave visible; you'll see the executor/proving lines on the next tx
```

**SAY:**
> "Now anchor the CID. The registry derives a program-derived account from the CID
> itself, stores the CID, its metadata hash, and the timestamp, and claims that
> account — idempotently. The transaction lands on-chain."

**SHOW:** run the anchor (single-CID `anchor-one`, which is what's wired through
the IDL CLI; narrate that `anchor_batch` does the same for ≥10 CIDs):
```
wb_registry_cli anchor-one \
  --cid "zDvTestWhistleblowerCID0001" \
  --metadata-hash <64-hex> \
  --anchor-timestamp <unix-ms> \
  --record <derived-PDA>
```
Point at the seq.log line showing the tx executed/proved and a new block created.

---

## Scene 6 — The registry confirms the registration  (~0:45)
*(LP-0017 demo item 3: "the on-chain registry confirming the CID registration.")*

**SAY:**
> "And to confirm it's really on-chain: I query the registry by CID. It derives
> the same account, reads it back, and decodes the stored record — the CID, the
> metadata hash, and the anchor timestamp. The document is now permanently
> indexed and discoverable."

**SHOW:**
```
wb_registry_cli inspect <derived-PDA> --type RegistryRecord
```
Point at the decoded `RegistryRecord { cid, metadata_hash, anchor_timestamp }`.

---

## Scene 7 — Wrap  (~0:30)

**SAY:**
> "To recap: an upload→broadcast→anchor pipeline that's censorship-resistant by
> construction — Storage holds the bytes, Delivery makes them instantly findable,
> and a permissionless, idempotent LEZ program anchors them for the long term.
> The indexing logic is a reusable module, the registry has a SPEL IDL and
> generated clients, and everything's MIT/Apache dual-licensed. Thanks for
> watching."

**SHOW:** the repo root (`README.md` + `SUBMISSION.md` criteria table), then the
license files.

---

## Appendix — exact commands & the real values captured on the build mini

Use these so the on-screen commands match real output. (From the verified run;
your CID/PDA/tx will differ per run.)

- **Program image id (on-chain address):**
  `[736378292, 3237769127, 3218962078, 1003268346, 3355061132, 654317770, 4171522436, 2002532608]`
- **Test CID:** `zDvTestWhistleblowerCID0001`
- **Derived PDA:** `76d5eUdWWRBf7SbcSQFHEmzD4bJKAjqbK3mAqwecR1Tg`
- **Anchor tx (example):** `32673a9feb866e56f3fb0ac7ad8136804cd82fd07f712953bc4c0318c0c17213`

Run context (on the mini): `source ~/.cargo/env; export PATH="$HOME/.risc0/bin:$PATH"`,
`export RISC0_DEV_MODE=0`, `NSSA_WALLET_HOME_DIR=~/lez-seq/wallet/configs/debug`,
program dir `~/lamda/crates/wb-registry-program`, `.bin` at
`target/riscv-guest/.../release/wb_registry.bin`, CLI via
`cargo run --bin wb_registry_cli -- --idl whistleblower_registry-idl.json -p <.bin> …`.

**Honesty notes for the recording (state them, don't hide them):**
- The single-CID `anchor-one` is submitted through the IDL CLI live; `anchor_batch`
  (the ≥10-CID path) is the same registry logic and is driven by the
  `wb-lez-registry` adapter (the SPEL CLI can't encode `Vec<struct>` args).
- If Storage/Delivery nodes aren't running for Scene 4, demo that beat with
  `scripts/dev-demo.sh` and say so.
- The on-chain anchor + query in Scenes 5–6 is the part that runs with real
  `RISC0_DEV_MODE=0` proving — keep that terminal evidence clearly on screen.
