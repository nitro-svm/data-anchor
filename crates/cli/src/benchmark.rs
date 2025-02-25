use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use bytesize::ByteSize;
use chrono::Utc;
use clap::Parser;
use futures::StreamExt;
use nitro_da_client::{BloberClient, BloberClientResult, FeeStrategy, Priority};
use rand::{Rng, RngCore};
use solana_sdk::{pubkey::Pubkey, signer::Signer};
use tracing::{instrument, trace};

#[derive(Debug, Parser)]
pub enum BenchmarkSubCommand {
    /// Generate data files with random bytes.
    #[command(visible_alias = "g")]
    GenerateData {
        /// The path where to generate the data.
        data_path: PathBuf,
        /// The size of each data file in bytes.
        #[arg(short, long, default_value_t = 1000)]
        size: u64,
        /// The number of data files to generate.
        #[arg(short, long, default_value_t = 100)]
        count: u64,
        /// Whether to randomize file length.
        #[arg(short, long, default_value_t = false)]
        random_length: bool,
    },
    /// Upload all data files and measure the upload speed and cost.
    #[command(visible_alias = "m")]
    Measure {
        /// The path from which to read the data.
        data_path: PathBuf,
        /// The timeout for individual uploads.
        #[arg(short, long, default_value_t = 60)]
        timeout: u64,
        /// Concurrent uploads.
        #[arg(short, long, default_value_t = 100)]
        concurrency: u64,
        /// The priority to use for the uploads.
        #[arg(short, long, value_enum, default_value_t = Priority::Medium)]
        priority: Priority,
    },
}

impl BenchmarkSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(&self, client: Arc<BloberClient>, blober: Pubkey) -> BloberClientResult {
        match self {
            BenchmarkSubCommand::GenerateData {
                data_path,
                size,
                count,
                random_length,
            } => {
                let mut rng = rand::thread_rng();

                delete_all_in_dir(&data_path).await?;

                let files = (0..*count)
                    .map(|i| {
                        let size = if *random_length {
                            rng.gen_range(1, size) as usize
                        } else {
                            *size as usize
                        };
                        let mut data = vec![0u8; size];
                        rng.fill_bytes(&mut data);
                        tokio::fs::write(data_path.join(format!("data-{i}.bin")), data)
                    })
                    .collect::<Vec<_>>();

                match futures::future::try_join_all(files).await {
                    Ok(_) => println!("Data generated successfully."),
                    Err(e) => eprintln!("Error generating data: {e:?}"),
                }
            }
            BenchmarkSubCommand::Measure {
                data_path,
                timeout,
                concurrency,
                priority,
            } => {
                let reads = data_path
                    .read_dir()?
                    .filter_map(|entry| {
                        let path = entry.ok()?.path();
                        path.is_file().then_some(tokio::fs::read(path))
                    })
                    .collect::<Vec<_>>();

                trace!("Reading data files...");
                let data = futures::future::try_join_all(reads).await?;

                let total_size = ByteSize(data.iter().map(|d| d.len() as u64).sum());
                let total_files = data.len();
                let total_txs = data
                    .iter()
                    .map(|d| d.len().div_ceil(blober::CHUNK_SIZE as usize))
                    .sum::<usize>()
                    + total_files * 2;
                trace!("Read {total_files} files with a total size of {total_size}");

                let start_balance = client
                    .rpc_client()
                    .get_balance(&client.payer().pubkey())
                    .await?;
                let start_time = tokio::time::Instant::now();

                let status = StatusData::new();

                futures::stream::iter(data)
                    .map(|blob_data| {
                        let status = status.clone();
                        let client = client.clone();

                        async move {
                            let (sent, uploaded, fail) = status.increment_sent();
                            log(sent, uploaded, fail, total_files);
                            let result = client
                                .upload_blob(
                                    &blob_data,
                                    FeeStrategy::BasedOnRecentFees(*priority),
                                    blober,
                                    Some(Duration::from_secs(*timeout)),
                                )
                                .await;
                            let (sent, uploaded, fail) = status.increment_status(result.is_ok());
                            log(sent, uploaded, fail, total_files);
                            result
                        }
                    })
                    .buffer_unordered(*concurrency as usize)
                    .collect::<Vec<BloberClientResult<_>>>()
                    .await
                    .into_iter()
                    .collect::<BloberClientResult<Vec<_>>>()?;

                let elapsed = start_time.elapsed().as_secs_f64();
                let bps = ByteSize((total_size.0 as f64 / elapsed).round() as u64);
                let end_balance = client
                    .rpc_client()
                    .get_balance(&client.payer().pubkey())
                    .await?;

                let balance_diff = start_balance - end_balance;

                println!();
                println!(
                    "------ Benchmark Results at {time} ------",
                    time = Utc::now()
                );
                println!(
                    "Priority in the {priority} percentile",
                    priority = priority.percentile()
                );
                println!("Uploaded {total_size} bytes in {elapsed}s via {total_txs} transactions");
                println!(
                    "Aproximate speed: {bps}/s ({tx_per_sec} tx/s)",
                    tx_per_sec = total_txs as f64 / elapsed
                );
                println!("Cost: {start_balance} - {end_balance} = {balance_diff} lamports");
                println!(
                    "Average cost per blob: {cost_per_blob} lamports",
                    cost_per_blob = balance_diff / total_files as u64
                );
                println!(
                    "Average cost per byte: {cost_per_byte} lamports",
                    cost_per_byte = balance_diff / total_size.0
                );
            }
        }
        Ok(())
    }
}

#[instrument(skip(dir), level = "debug", fields(data_path = %dir.as_ref().display()))]
async fn delete_all_in_dir<P: AsRef<Path>>(dir: P) -> tokio::io::Result<()> {
    let mut read_dir = tokio::fs::read_dir(&dir).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            tokio::fs::remove_file(&path).await?;
        } else if path.is_dir() {
            tokio::fs::remove_dir_all(&path).await?;
        }
    }
    Ok(())
}

struct StatusData {
    sent: AtomicUsize,
    completed: AtomicUsize,
    failed: AtomicUsize,
}

impl StatusData {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            sent: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
        })
    }

    fn increment_sent(&self) -> (usize, usize, usize) {
        (
            self.sent.fetch_add(1, Ordering::SeqCst) + 1,
            self.completed.load(Ordering::SeqCst),
            self.failed.load(Ordering::SeqCst),
        )
    }

    fn increment_status(&self, success: bool) -> (usize, usize, usize) {
        if success {
            (
                self.sent.load(Ordering::SeqCst),
                self.completed.fetch_add(1, Ordering::SeqCst) + 1,
                self.failed.load(Ordering::SeqCst),
            )
        } else {
            (
                self.sent.load(Ordering::SeqCst),
                self.completed.load(Ordering::SeqCst),
                self.failed.fetch_add(1, Ordering::SeqCst) + 1,
            )
        }
    }
}

fn log(sent: usize, completed: usize, failed: usize, total_files: usize) {
    print!("\rSent {sent} | Uploaded {completed} | Failed {failed} | Total {total_files}");
    std::io::stdout().flush().unwrap();
}
