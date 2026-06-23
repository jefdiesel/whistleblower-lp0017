//! SPEL-provided CLI for the program: builds/submits transactions and inspects
//! accounts using the generated IDL. Run via `make cli ARGS="..."`.
#[tokio::main]
async fn main() {
    spel::run().await;
}
