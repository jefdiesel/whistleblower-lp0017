//! Shared CLI definition and the generic `run` driver for the Whistleblower
//! batch-anchor tool.
//!
//! The same `Cli` + `run` are used by two binaries that differ only in which
//! [`RegistryClient`](wb_index::RegistryClient) they construct:
//!
//! * `wb-batch-anchor` (this crate) — uses a file-backed registry for local/dev
//!   demos and CI; no sequencer required.
//! * `wb-batch-anchor-lez` (the `wb-lez-registry` crate) — uses the real
//!   on-chain `LezRegistry`.
//!
//! Because everything is generic over `RegistryClient`, the permissionless
//! accumulate → batch → checkpoint → resume logic lives in one place.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use wb_index::{
    BatchAnchorRunner, CheckpointStore, HttpDelivery, HttpStorage, PublishMeta, Publisher,
    RegistryClient, RunnerConfig,
};

/// Permissionless Whistleblower batch-anchor tool.
#[derive(Parser, Debug)]
#[command(name = "wb-batch-anchor", version, about, long_about = None)]
pub struct Cli {
    /// Logos Delivery (Waku) REST endpoint.
    #[arg(
        long,
        env = "WB_DELIVERY_URL",
        default_value = "http://127.0.0.1:8645",
        global = true
    )]
    pub delivery_url: String,

    /// Logos Storage (Codex) REST endpoint, including the API prefix.
    #[arg(
        long,
        env = "WB_STORAGE_URL",
        default_value = "http://localhost:8080/api/storage/v1",
        global = true
    )]
    pub storage_url: String,

    /// Delivery content topic to publish/subscribe.
    #[arg(long, env = "WB_TOPIC", default_value_t = wb_types::topic::documents_content_topic(), global = true)]
    pub topic: String,

    /// Checkpoint file enabling resume across restarts.
    #[arg(
        long,
        env = "WB_CHECKPOINT",
        default_value = ".wb/checkpoint.json",
        global = true
    )]
    pub checkpoint: PathBuf,

    /// File-backed registry path (local/dev binary only; ignored on-chain).
    #[arg(
        long,
        env = "WB_REGISTRY_FILE",
        default_value = ".wb/registry.json",
        global = true
    )]
    pub registry_file: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Upload a file to Logos Storage and broadcast its metadata envelope.
    Publish {
        /// Path to the file to publish.
        file: PathBuf,
        /// Document title (defaults to the file name).
        #[arg(long)]
        title: Option<String>,
        /// Document description.
        #[arg(long, default_value = "")]
        description: String,
        /// Comma-separated tags.
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        /// Override the inferred MIME type.
        #[arg(long)]
        content_type: Option<String>,
    },
    /// Run the batch-anchor daemon: subscribe, accumulate, and anchor in batches
    /// until interrupted (Ctrl-C).
    Run {
        /// Flush as soon as this many tuples are pending (>= 10 required on-chain).
        #[arg(long, default_value_t = 50)]
        batch_size: usize,
        /// How often to drain the topic, in seconds.
        #[arg(long, default_value_t = 2)]
        poll_secs: u64,
        /// Flush a partial batch on this cadence, in seconds.
        #[arg(long, default_value_t = 10)]
        flush_secs: u64,
    },
    /// One-shot: drain the topic for a few rounds, anchor whatever accumulated,
    /// then exit. Handy for demos and CI.
    Anchor {
        /// Number of poll rounds before flushing.
        #[arg(long, default_value_t = 3)]
        rounds: usize,
        /// Delay between rounds, in seconds.
        #[arg(long, default_value_t = 1)]
        poll_secs: u64,
    },
    /// Query the registry for a CID.
    Query {
        cid: String,
        /// Emit JSON instead of a human-readable record.
        #[arg(long)]
        json: bool,
    },
    /// Show checkpoint status (anchored count, batches, last tx).
    Status,
}

/// Initialize tracing/logging from `RUST_LOG` (default `info`).
pub fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let _ = fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .try_init();
}

/// Drive the CLI with a concrete registry implementation.
pub async fn run<R: RegistryClient>(cli: Cli, registry: R) -> anyhow::Result<()> {
    match &cli.command {
        Command::Publish {
            file,
            title,
            description,
            tags,
            content_type,
        } => {
            publish(
                &cli,
                file,
                title.clone(),
                description.clone(),
                tags.clone(),
                content_type.clone(),
            )
            .await
        }
        Command::Run {
            batch_size,
            poll_secs,
            flush_secs,
        } => run_daemon(&cli, registry, *batch_size, *poll_secs, *flush_secs).await,
        Command::Anchor { rounds, poll_secs } => {
            anchor_once(&cli, registry, *rounds, *poll_secs).await
        }
        Command::Query { cid, json } => query(registry, cid, *json).await,
        Command::Status => status(&cli).await,
    }
}

async fn publish(
    cli: &Cli,
    file: &PathBuf,
    title: Option<String>,
    description: String,
    tags: Vec<String>,
    content_type: Option<String>,
) -> anyhow::Result<()> {
    let bytes = tokio::fs::read(file)
        .await
        .with_context(|| format!("reading {}", file.display()))?;
    let filename = file.file_name().map(|n| n.to_string_lossy().to_string());
    let title = title
        .or_else(|| filename.clone())
        .unwrap_or_else(|| "untitled".to_string());

    let publisher = Publisher::new(
        HttpStorage::new(&cli.storage_url),
        HttpDelivery::new(&cli.delivery_url),
    )
    .content_topic(cli.topic.clone());

    let outcome = publisher
        .publish(
            &bytes,
            PublishMeta {
                title,
                description,
                content_type,
                filename,
                tags,
            },
        )
        .await
        .context("publish failed")?;

    println!("Published:");
    println!("  CID:           {}", outcome.cid);
    println!(
        "  metadata_hash: {}",
        hex::encode(outcome.envelope.metadata_hash())
    );
    println!("  content_type:  {}", outcome.envelope.content_type);
    println!("  size_bytes:    {}", outcome.envelope.size_bytes);
    println!("  topic:         {}", cli.topic);
    println!(
        "  broadcast:     {}",
        if outcome.broadcast {
            "yes"
        } else {
            "deduplicated (already broadcast)"
        }
    );
    Ok(())
}

fn build_runner<R: RegistryClient>(
    cli: &Cli,
    registry: R,
    config: RunnerConfig,
) -> BatchAnchorRunner<HttpDelivery, R> {
    BatchAnchorRunner::new(
        HttpDelivery::new(&cli.delivery_url),
        registry,
        CheckpointStore::new(&cli.checkpoint),
        config,
    )
}

async fn run_daemon<R: RegistryClient>(
    cli: &Cli,
    registry: R,
    batch_size: usize,
    poll_secs: u64,
    flush_secs: u64,
) -> anyhow::Result<()> {
    let config = RunnerConfig {
        content_topic: cli.topic.clone(),
        batch_size,
        poll_interval: Duration::from_secs(poll_secs.max(1)),
        flush_interval: Duration::from_secs(flush_secs.max(1)),
    };
    let mut runner = build_runner(cli, registry, config);

    println!(
        "Anchoring topic {} -> registry (batch_size={batch_size}). Press Ctrl-C to stop.",
        cli.topic
    );
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutdown signal received; flushing");
    };
    let stats = runner.run(shutdown).await.context("runner failed")?;
    println!(
        "Stopped. received={} accepted={} duplicates={} invalid={} anchored={} batches={}",
        stats.received,
        stats.accepted,
        stats.duplicates,
        stats.invalid,
        stats.anchored,
        stats.batches
    );
    Ok(())
}

async fn anchor_once<R: RegistryClient>(
    cli: &Cli,
    registry: R,
    rounds: usize,
    poll_secs: u64,
) -> anyhow::Result<()> {
    let config = RunnerConfig {
        content_topic: cli.topic.clone(),
        batch_size: usize::MAX, // never auto-flush; we flush explicitly at the end
        ..RunnerConfig::default()
    };
    let mut runner = build_runner(cli, registry, config);
    runner.init().await.context("init failed")?;

    for round in 0..rounds.max(1) {
        let n = runner.poll_once().await.context("poll failed")?;
        println!("round {}: accepted {} new tuple(s)", round + 1, n);
        if n == 0 && round > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_secs(poll_secs.max(1))).await;
    }

    match runner.flush().await.context("anchor failed")? {
        Some(receipt) => {
            println!(
                "Anchored batch {}: {} new, {} already present",
                receipt.tx_hash,
                receipt.anchored.len(),
                receipt.already_present.len()
            );
            for cid in &receipt.anchored {
                println!("  + {cid}");
            }
        }
        None => println!("Nothing to anchor."),
    }
    Ok(())
}

async fn query<R: RegistryClient>(registry: R, cid: &str, json: bool) -> anyhow::Result<()> {
    match registry.get_by_cid(cid).await.context("query failed")? {
        Some(rec) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&rec)?);
            } else {
                println!("CID:             {}", rec.cid);
                println!("metadata_hash:   {}", hex::encode(rec.metadata_hash));
                println!("anchor_timestamp: {} ms", rec.anchor_timestamp);
            }
        }
        None => {
            println!("CID not found in registry: {cid}");
            std::process::exit(2);
        }
    }
    Ok(())
}

async fn status(cli: &Cli) -> anyhow::Result<()> {
    let store = CheckpointStore::new(&cli.checkpoint);
    let cp = store.load().await.context("loading checkpoint")?;
    println!("Checkpoint: {}", store.path().display());
    println!("  anchored CIDs:   {}", cp.anchored_cids.len());
    println!("  batches:         {}", cp.batches_committed);
    println!(
        "  last batch tx:   {}",
        cp.last_batch_tx.as_deref().unwrap_or("(none)")
    );
    Ok(())
}
