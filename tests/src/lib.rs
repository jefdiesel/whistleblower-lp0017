//! integration tests live in tests/
//!
//! This crate (`wb-e2e`) carries no library code of its own; it exists only as a
//! home for the end-to-end integration tests under `tests/`, which drive the
//! full Whistleblower pipeline (Publisher + BatchAnchorRunner + registry) through
//! in-process fakes. See `tests/e2e.rs`.
