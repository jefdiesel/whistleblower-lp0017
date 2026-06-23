//! Generate the IDL JSON for the `whistleblower_registry` program.
//!
//! Usage:
//!   cargo run --bin generate_idl > whistleblower_registry-idl.json
//!
//! The macro reads the guest program source at compile time, finds the
//! `#[lez_program]` module and its `#[account_type]` structs, and expands into a
//! `main` that prints the IDL. `make idl` runs this.
spel_framework::generate_idl!("../methods/guest/src/bin/wb_registry.rs");
