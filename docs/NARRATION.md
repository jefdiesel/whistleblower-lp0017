# Whistleblower (LP-0017) — narration script (plain speech)

Word-for-word voice-over, written to be *said* out loud (~4–5 min). It covers
what the prize asks for: **what** you built, **why**, the **architecture and key
decisions**, and a walkthrough of the **live demo**. Pause where it says `[pause]`
to let the terminal output land on screen.

---

## 1. What it is and why (~45s)

> "I'm going to walk you through **Whistleblower**, which I built for Logos prize
> LP-0017. The idea is simple but it matters: a whistleblower or a journalist
> needs to publish a document that **can't be quietly taken down**. Today the
> options are bad — you trust a host that can be pressured, you pay on-chain fees
> up front, or you have no guarantee the document stays findable. Whistleblower
> fixes that with three layers of the Logos stack: **Storage** holds the file,
> **Delivery** broadcasts it so it's instantly findable peer-to-peer, and an
> on-chain **registry** anchors it permanently — with no single point anyone can
> censor."

## 2. What I built (~50s)

> "There are four pieces. **One**, a reusable *document-indexing module* — the
> upload-broadcast-anchor logic, packaged so any Logos app can use it, not just
> this one. **Two**, a *permissionless batch-anchor tool* — and permissionless is
> the key word: anyone can run it. An NGO, a journalist collective, an automated
> watcher — they collect the broadcast document IDs and commit them on-chain in a
> single transaction, with zero coordination with whoever originally published.
> The publisher doesn't even need to hold tokens. **Three**, the on-chain
> *registry* itself, written as a program on the Logos Execution Zone. And
> **four**, the *Basecamp app* — the GUI a user actually clicks."

## 3. The key decisions (~75s)

> "A few decisions worth calling out. The prize let me choose between writing a
> **LEZ program** or inscribing directly to consensus with the zone SDK. I chose
> the program, deliberately — the zone-SDK path currently needs one *designated
> actor* to write to consensus, and that single actor is exactly the censorship
> choke-point this whole project exists to avoid. A program encodes the rules as
> code anyone can audit and call.
> [pause]
> Inside the program, **every document gets its own account, derived
> deterministically from its content ID** — so anyone can find a document's
> on-chain record straight from its CID, with no index to maintain.
> It's **idempotent**: if two people anchor the same document, or someone re-runs
> the tool, it just succeeds — no duplicates, no errors. And the batch tool
> **checkpoints** its progress, so if the network drops mid-run it resumes without
> redoing work.
> One more: the **metadata hash is a canonical, language-agnostic format**, so the
> Rust tools and the C++ GUI produce the identical hash — they agree by
> construction."

## 4. The live demo (~90s) — say this as it runs

> "Now let me show it actually working — and this is real: the sequencer is
> running with **`RISC0_DEV_MODE=0`**, so these are real zero-knowledge proofs,
> not dev-mode mocks.
> [pause]
> First I **upload a document to Logos Storage**, and it returns a content ID — a
> CID. That CID is what makes the document findable. [pause]
> I **broadcast that CID and its metadata** to the Whistleblower Delivery topic,
> and a subscriber on that topic sees it **instantly** — peer to peer, no server.
> [pause]
> Now the anchoring. The tool **derives the on-chain account** for this CID and
> submits the anchor transaction. Watch the terminal — the transaction is
> submitted, **proven, and confirmed in a block.** It's permanently on-chain now.
> [pause]
> And to prove it's really there, I **query the registry by the CID** — it derives
> the same account, reads it back, and there's the record: the CID, the metadata
> hash, and the timestamp. A full round trip — uploaded, broadcast, anchored, and
> queryable."

## 5. Wrap (~25s)

> "So that's Whistleblower — censorship-resistant by construction. Storage keeps
> the bytes, Delivery makes them instantly findable, and a permissionless,
> idempotent on-chain program anchors them for good. The indexing logic is a
> reusable module, the registry ships a generated IDL and clients, and it's all
> open source under MIT and Apache 2.0. Thanks for watching."

---

### Delivery notes
- Speak slowly during the terminal beats; let each command's output finish before
  the next sentence.
- The prize requires the recording to **visibly show proof generation with
  `RISC0_DEV_MODE=0`** — keep that terminal line on screen during section 4.
- The prize's required demo beats are: (a) a file uploaded and **findable via the
  Delivery topic**, (b) the **batch tool picking up the broadcast CID** and
  anchoring it, (c) the **registry confirming** the registration. Make sure the
  take shows the broadcast → pickup → anchor path, not only a direct anchor.
