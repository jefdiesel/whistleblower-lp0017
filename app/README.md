<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Whistleblower — Basecamp app (LP-0017)

A censorship-resistant document upload app for **Logos Basecamp**. You pick a
file; the app uploads its bytes to **Logos Storage**, broadcasts a JSON metadata
envelope over **Logos Delivery** (so the document is immediately findable by
anyone subscribed to the topic), and — as a *separate, optional* step — anchors a
`(cid, metadata_hash)` tuple **on-chain** via the registry module.

This directory is the GUI module only. The data shapes, canonical hashing, the
permissionless batch-anchor tool, and the indexer live in the Rust crates under
[`../crates`](../crates).

## What it is

A Logos Core module of type **`ui_qml`**: a QML view (`qml/Main.qml`) plus a
small C++/Qt6 backend (`src/WhistleblowerBackend.*`), packaged as a `.lgx` with
Nix and loaded in Logos Basecamp. The module manifest is
[`metadata.json`](./metadata.json).

The QML engine runs in a **network sandbox** and performs no IO itself.
**Everything network/storage/chain happens through Logos Core modules** via the
injected `logos` bridge (`callModuleAsync` / `callModule` / `onModuleEvent`). The
C++ backend only does pure local work: canonical metadata hashing, envelope JSON
assembly, and reading the clock.

### Canonical metadata hash

`WhistleblowerBackend::metadataHash` is a byte-for-byte port of the Rust
canonical algorithm in
[`../crates/wb-types/src/hash.rs`](../crates/wb-types/src/hash.rs). The two
implementations **must stay byte-identical** — the on-chain registry stores this
hash and the Rust batch-anchor tool recomputes it from the broadcast envelope, so
any divergence makes a document un-anchorable / unverifiable. A shared test
vector is documented at the top of `src/WhistleblowerBackend.cpp`; cross-check
against the Rust tests when validating.

## Dependencies

This module declares three Logos Core module dependencies in `metadata.json`:

| Module                   | Used for                                                   |
| ------------------------ | --------------------------------------------------------- |
| `storage_module`         | `uploadUrl` (and `downloadToUrl`/`exists`/`space`)        |
| `delivery_module`        | `createNode` / `start` / `subscribe` / `send` + events    |
| `whistleblower_registry` | `anchorBatch` (the optional on-chain anchor)              |

Install them into your Logos environment before loading the app, e.g.:

```sh
# Download a published package, then install it (names/IDs per your registry):
lgpd download storage_module
lgpd download delivery_module
lgpm install storage_module
lgpm install delivery_module
```

`whistleblower_registry` is **generated**, not published like the others: it is
produced by

```sh
spel-client-gen --target logos-module   # from the registry program's IDL
```

which emits a Logos Core module wrapping the on-chain program (it exposes the
`anchorBatch` method the GUI calls). Build/install that module alongside the
others. See [`../crates`](../crates) and the registry program's README for the
program/IDL.

## Build

Requires **Nix** (flakes enabled) and **Qt6** — this app **cannot be built
without them**. The `flake.nix` here is based on the logos-module-builder
template (`nix flake init -t github:logos-co/logos-module-builder/tutorial-v1`)
and may need small alignment to the installed logos-module-builder API (see the
comments in `flake.nix` and `CMakeLists.txt`).

```sh
cd app

# Build the module:
nix build

# Build the packaged .lgx for Basecamp:
nix build .#lgx
# -> ./result (a symlink to the built .lgx)

# Optional: a dev shell with cmake/ninja/qt6 for local iteration:
nix develop
```

The build is driven by `CMakeLists.txt`, which uses the `logos_module()` CMake
macro from logos-module-builder (C++17, finds `Qt6 Core Qml Quick`, embeds the
`qml/` resources, and registers `WhistleblowerBackend` as the QML type
`Whistleblower 1.0`).

## Load into Basecamp

1. Open **Logos Basecamp**.
2. In the **Package Manager** panel, add/import the dependency modules
   (`storage_module`, `delivery_module`, `whistleblower_registry`) and this
   app's `.lgx` (from `nix build .#lgx`).
3. In the **Modules** panel, enable the modules, then launch **Whistleblower**.

## Using it

1. **Choose file…** — pick a local document.
2. Fill in **Title**, **Description**, and comma-separated **Tags**.
3. **Upload & Broadcast** — uploads bytes to Storage; on completion the app shows
   the returned **CID** and broadcasts the metadata envelope to
   `"/whistleblower/1/documents/json"`. Other instances see it appear under
   *"Documents seen on the network"* — demonstrating immediate discoverability
   via Delivery.
4. **Anchor on-chain** *(optional, separate button — enabled only once a CID
   exists)* — computes the canonical `metadata_hash` over the same inputs that
   were broadcast and submits `anchorBatch([{cid, metadata_hash}], now_ms)` to
   `whistleblower_registry`.

## Layout

```
app/
├── metadata.json              # module manifest (type=ui_qml, deps)
├── flake.nix                  # Nix build (module + .lgx)
├── CMakeLists.txt             # logos_module() build, Qt6, QML type registration
├── qml/
│   └── Main.qml               # the GUI, wired to the `logos` bridge
└── src/
    ├── WhistleblowerBackend.h
    └── WhistleblowerBackend.cpp   # canonical hash (mirrors wb-types/src/hash.rs)
```

## License

Dual-licensed under **MIT OR Apache-2.0** — see
[`../LICENSE-MIT`](../LICENSE-MIT) and [`../LICENSE-APACHE`](../LICENSE-APACHE).
