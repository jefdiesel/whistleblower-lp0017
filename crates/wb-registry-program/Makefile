IDL_FILE        ?= whistleblower_registry-idl.json
GUEST_MANIFEST   = methods/guest/Cargo.toml
PROGRAM_BIN_DOCKER = methods/guest/target/riscv32im-risc0-zkvm-elf/docker/wb_registry.bin
PROGRAM_BIN_LOCAL  = methods/guest/target/riscv32im-risc0-zkvm-elf/release/wb_registry
PROGRAM_BIN      ?= $(PROGRAM_BIN_DOCKER)
SPEL_CLIENT_GEN  ?= spel-client-gen

.PHONY: build build-local idl deploy cli ffi-gen ui-gen clean

## Reproducible guest build (requires Docker).
build:
	cargo risczero build --manifest-path $(GUEST_MANIFEST)

## No-Docker guest build using the installed RISC0 rust toolchain.
build-local:
	cd methods/guest && cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf

## Generate the program IDL JSON (SPEL).
idl:
	cargo run --bin generate_idl > $(IDL_FILE)

## Deploy the compiled program; prints the program id (= RISC0 image id).
deploy:
	wallet deploy-program $(PROGRAM_BIN)

## Run the SPEL-generated CLI (e.g. make cli ARGS="anchor-batch ...").
cli:
	cargo run --bin wb_registry_cli -- $(ARGS)

## Generate the Rust+FFI client consumed by the on-chain CLI adapter (wb-lez-registry).
ffi-gen:
	$(SPEL_CLIENT_GEN) --idl $(IDL_FILE) --out-dir generated/rust-ffi --target rust+ffi

## Generate the Qt/QML Basecamp module consumed by the app.
ui-gen:
	$(SPEL_CLIENT_GEN) --idl $(IDL_FILE) --out-dir ../../app/generated --target logos-module --module-name whistleblower_registry

clean:
	cargo clean
	cd methods/guest && cargo clean
