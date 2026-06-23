<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Whistleblower (LP-0017) — per-clip narration (record one file per clip)

Read each clip aloud, **record it as its own file** (`clip1`…`clip7`, any of
`.m4a/.mp3/.wav` — a phone voice memo is fine). Read naturally; leave ~1 second of
silence at the start and end of each. The on-screen visuals will be stretched to
match your audio, so you don't have to match any timing.

Drop the files in `/Users/jef/lamda/narration/` (or AirDrop to the laptop and tell
me the path), then say "audio's in".

---

## Clip 1 — Title
> This is Whistleblower — my entry for Lambda Prize seventeen. It's a
> censorship-resistant way to publish documents and register them on-chain, built
> on the Logos stack. And everything you're about to see is real, running
> on-chain — actual RISC-Zero execution, with dev mode turned off.

## Clip 2 — Upload to Storage
> First I upload a document to Logos Storage — that's Codex. It comes back with a
> content ID, a CID, derived from the file itself. That CID is the fingerprint
> we're going to anchor.

## Clip 3 — Anchor one CID
> Now I anchor that CID on-chain. This runs the registry program inside the
> RISC-Zero zkVM — real execution, dev mode zero. You can see the transaction
> hash, and then I read the record straight back from its own on-chain account.
> One of one, verified.

## Clip 4 — Batch-anchor 12
> Anchoring one at a time works, but the prize asks for a batch tool. So here I
> anchor twelve CIDs in a single transaction. All twelve land, and I read every
> one of them back from chain — twelve of twelve.

## Clip 5 — Batch-anchor 50
> And here's the target — fifty CIDs in one transaction. Fifty of fifty records,
> verified on-chain. And it's idempotent: re-anchoring a CID that already exists
> just passes through, so the tool is safe to stop and resume.

## Clip 6 — Benchmarks
> So how fast is it? These are real executor times from the sequencer log. A
> single CID is about three milliseconds; fifty CIDs is about forty-eight. That's
> under one millisecond per CID in a batch — roughly three times cheaper per
> document than anchoring them one by one. That's the whole point of batching.

## Clip 7 — Wrap
> So that's Whistleblower: a reusable document-indexing module, an on-chain
> registry on the Logos Execution Zone, and a permissionless, resumable
> batch-anchor tool — all dual-licensed. The code is public at the link on screen.
> Thanks for watching.
