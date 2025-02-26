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
use chrono::{DateTime, Utc};
use clap::Parser;
use futures::StreamExt;
use itertools::iproduct;
use nitro_da_client::{BloberClient, BloberClientResult, FeeStrategy, Priority};
use rand::{Rng, RngCore};
use serde::Serialize;
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
    /// Automate the benchmarking process.
    #[command(visible_alias = "a")]
    Automate {
        /// The path where to generate the data.
        data_path: PathBuf,
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
                generate_data(data_path, *count as usize, *random_length, *size as usize).await?;
            }
            BenchmarkSubCommand::Measure {
                data_path,
                timeout,
                concurrency,
                priority,
            } => {
                println!(
                    "\n{}",
                    write_measurements(vec![
                        measure_performance(
                            data_path,
                            *timeout,
                            *concurrency,
                            *priority,
                            client,
                            blober,
                        )
                        .await?,
                    ])?
                );
            }
            BenchmarkSubCommand::Automate { data_path } => {
                let mut measurements = Vec::new();
                // Generate data files with different sizes and counts.
                let combination_matrix =
                    iproduct!([100, 1000, 10000], [10, 100, 1000], [false, true]);
                let priorities = [
                    Priority::Min,
                    Priority::Low,
                    Priority::Medium,
                    Priority::High,
                    Priority::VeryHigh,
                ];

                let _: BloberClientResult = async {
                    for (size, count, random_length) in combination_matrix {
                        println!(
                            "Generating data files with size {size}{} and count {count}...",
                            if random_length {
                                " (random length)"
                            } else {
                                ""
                            }
                        );
                        generate_data(data_path, count, random_length, size).await?;
                        for priority in priorities {
                            println!(
                                "Measuring performance with percentile priority {}...",
                                priority.percentile()
                            );
                            measurements.push(
                                measure_performance(
                                    data_path,
                                    600,
                                    100,
                                    priority,
                                    client.clone(),
                                    blober,
                                )
                                .await?,
                            );
                        }
                    }
                    Ok(())
                }
                .await;
                delete_all_in_dir(data_path).await?;
                println!("\n{}", write_measurements(measurements)?);
            }
        }
        Ok(())
    }
}

/// Generates data for benchmarking.
async fn generate_data(
    data_path: &Path,
    count: usize,
    random_length: bool,
    size: usize,
) -> BloberClientResult {
    let mut rng = rand::thread_rng();

    delete_all_in_dir(data_path).await?;

    let files = (0..count)
        .map(|i| {
            let size = if random_length {
                rng.gen_range(1, size)
            } else {
                size
            };
            let mut data = vec![0u8; size];
            rng.fill_bytes(&mut data);
            (data_path.join(format!("data-{i}.bin")), data)
        })
        .collect::<Vec<_>>();

    // We buffer to avoid opening too many files at once.
    match futures::stream::iter(files)
        .map(|(path, data)| tokio::fs::write(path, data))
        .buffer_unordered(300)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(_) => println!("Data generated successfully."),
        Err(e) => eprintln!("Error generating data: {e:?}"),
    }
    Ok(())
}

/// Measures the performance of the blober.
async fn measure_performance(
    data_path: &Path,
    timeout: u64,
    concurrency: u64,
    priority: Priority,
    client: Arc<BloberClient>,
    blober: Pubkey,
) -> BloberClientResult<BenchMeasurement> {
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
                        FeeStrategy::BasedOnRecentFees(priority),
                        blober,
                        Some(Duration::from_secs(timeout)),
                    )
                    .await;
                let (sent, uploaded, fail) = status.increment_status(result.is_ok());
                log(sent, uploaded, fail, total_files);
                result
            }
        })
        .buffer_unordered(concurrency as usize)
        .collect::<Vec<BloberClientResult<_>>>()
        .await
        .into_iter()
        .collect::<BloberClientResult<Vec<_>>>()?;

    let elapsed = start_time.elapsed();
    let end_balance = client
        .rpc_client()
        .get_balance(&client.payer().pubkey())
        .await?;

    println!();
    Ok(BenchMeasurement::new(
        priority.percentile(),
        elapsed,
        total_size,
        total_txs,
        start_balance,
        end_balance,
        total_files,
    ))
}

/// Writes a list of measurements to a CSV string.
fn write_measurements(measurements: Vec<BenchMeasurement>) -> BloberClientResult<String> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    for measurement in measurements {
        writer.serialize(measurement).unwrap();
    }
    Ok(String::from_utf8(writer.into_inner().unwrap()).unwrap())
}

/// Deletes all files and directories in a directory.
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

/// A measurement of the performance of the blober.
#[derive(Debug, Serialize)]
struct BenchMeasurement {
    timestamp: DateTime<Utc>,
    priority: f32,
    elapsed: f64,
    #[serde(serialize_with = "serialize_byte_size")]
    total_size: ByteSize,
    #[serde(serialize_with = "serialize_byte_size")]
    bps: ByteSize,
    total_txs: usize,
    tps: f64,
    start_balance: u64,
    end_balance: u64,
    total_cost: u64,
    cost_per_byte: u64,
    total_files: usize,
    cost_per_blob: u64,
}

/// Serialize a [`ByteSize`] to a string.
fn serialize_byte_size<S: serde::Serializer>(
    size: &ByteSize,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&size.to_string())
}

impl BenchMeasurement {
    fn new(
        priority: f32,
        elapsed: Duration,
        total_size: ByteSize,
        total_txs: usize,
        start_balance: u64,
        end_balance: u64,
        total_files: usize,
    ) -> Self {
        let balance_diff = start_balance - end_balance;
        let elapsed = elapsed.as_secs_f64();
        Self {
            timestamp: Utc::now(),
            priority,
            elapsed,
            total_size,
            bps: ByteSize((total_size.0 as f64 / elapsed).round() as u64),
            total_txs,
            tps: total_txs as f64 / elapsed,
            start_balance,
            end_balance,
            total_cost: balance_diff,
            cost_per_byte: balance_diff / total_size.0,
            total_files,
            cost_per_blob: balance_diff / total_files as u64,
        }
    }
}

/// Shared data for tracking the status of uploads.
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

    /// Increments the counter for sent uploads.
    fn increment_sent(&self) -> (usize, usize, usize) {
        (
            self.sent.fetch_add(1, Ordering::SeqCst) + 1,
            self.completed.load(Ordering::SeqCst),
            self.failed.load(Ordering::SeqCst),
        )
    }

    /// Increments the counters for completed and failed uploads.
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

/// Logs progress when benchmarking.
fn log(sent: usize, completed: usize, failed: usize, total_files: usize) {
    print!("\rSent {sent} | Uploaded {completed} | Failed {failed} | Total {total_files}");
    std::io::stdout().flush().unwrap();
}
