use std::{path::PathBuf, sync::Arc};

use anchor_lang::{prelude::Pubkey, solana_program::clock::Slot};
use clap::Parser;
use data_anchor_api::pubkey_with_str;
use data_anchor_client::{
    DataAnchorClient, DataAnchorClientResult, FeeStrategy, Priority, TransactionType,
};
use data_anchor_utils::{compression::DataAnchorCompressionAsync, encoding::DataAnchorEncoding};
use itertools::Itertools;
use serde::Serialize;
use solana_signature::Signature;
use tokio::io::AsyncReadExt;
use tracing::instrument;

use crate::formatting::CommandOutput;

#[derive(Debug, Parser)]
pub enum BlobSubCommand {
    /// Upload a blob of data. If no arguments are provided, the data will be read from stdin.
    #[command(visible_alias = "u")]
    Upload {
        /// The path to the data to upload.
        #[arg(short, long)]
        data_path: Option<PathBuf>,

        /// The raw hex encoded data to upload.
        #[arg(long, conflicts_with = "data_path")]
        data: Option<String>,
    },
    /// Discard a blob.
    #[command(visible_alias = "d")]
    Discard {
        /// The Pubkey of the blob to discard.
        blob: Pubkey,
    },
    /// Fetch blob data from the ledger.
    #[command(visible_alias = "f")]
    Fetch {
        /// The signatures of the transactions from which the blob data will be fetched.
        signatures: Vec<Signature>,
    },
    /// Get all blobs finalized in the given slot.
    #[command(visible_alias = "g")]
    Get {
        /// The slot to get blobs from.
        slot: Slot,
        /// The number of slots to look back to find all pieces of the finalized blobs.
        #[arg(short, long)]
        lookback_slots: Option<u64>,
    },
}

#[derive(Debug, Serialize)]
pub enum BlobCommandOutput {
    Posting {
        slot: Slot,
        #[serde(with = "pubkey_with_str")]
        address: Pubkey,
        signatures: Vec<Signature>,
        success: bool,
    },
    Fetching(Vec<Vec<u8>>),
}

impl std::fmt::Display for BlobCommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlobCommandOutput::Fetching(blobs) => {
                write!(
                    f,
                    "Fetched blobs: [{}]",
                    blobs.iter().map(hex::encode).collect_vec().join(", ")
                )
            }
            BlobCommandOutput::Posting {
                slot,
                address,
                signatures,
                success,
            } => {
                write!(
                    f,
                    "Slot: {slot}, Address: {address}, Signatures: [{}], Success: {success}",
                    signatures
                        .iter()
                        .map(|sig| sig.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            }
        }
    }
}

impl BlobSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run<Encoding, Compression>(
        &self,
        client: Arc<DataAnchorClient<Encoding, Compression>>,
        namespace: &str,
    ) -> DataAnchorClientResult<CommandOutput>
    where
        Encoding: DataAnchorEncoding + Default,
        Compression: DataAnchorCompressionAsync,
    {
        match self {
            BlobSubCommand::Upload { data_path, data } => {
                let blob_data = if let Some(data_path) = data_path {
                    tokio::fs::read(data_path)
                        .await
                        .unwrap_or_else(|_| panic!("failed to read file at {data_path:?}"))
                } else if let Some(data) = data {
                    hex::decode(data).unwrap_or_else(|_| panic!("failed to decode hex data"))
                } else {
                    let mut input = tokio::io::stdin();
                    let mut data = String::new();
                    input
                        .read_to_string(&mut data)
                        .await
                        .unwrap_or_else(|_| panic!("failed to read from stdin"));
                    data.into_bytes()
                };
                let (results, address) = client
                    .upload_blob(
                        &blob_data,
                        FeeStrategy::BasedOnRecentFees(Priority::VeryHigh),
                        namespace,
                        None,
                    )
                    .await?;
                let last_tx = results.last().expect("there should be at least one result");
                Ok(BlobCommandOutput::Posting {
                    slot: last_tx.slot,
                    address,
                    signatures: results.iter().map(|tx| tx.signature).collect(),
                    success: !matches!(last_tx.data, TransactionType::DiscardBlob),
                }
                .into())
            }
            BlobSubCommand::Discard { blob } => {
                let (results, _) = client
                    .discard_blob(
                        FeeStrategy::BasedOnRecentFees(Priority::VeryHigh),
                        *blob,
                        namespace,
                        None,
                    )
                    .await?;
                let last_tx = results.last().expect("there should be at least one result");
                Ok(BlobCommandOutput::Posting {
                    slot: last_tx.slot,
                    address: *blob,
                    signatures: results.iter().map(|tx| tx.signature).collect(),
                    success: matches!(last_tx.data, TransactionType::DiscardBlob),
                }
                .into())
            }
            BlobSubCommand::Fetch { signatures } => {
                let blob = client
                    .get_ledger_blobs_from_signatures::<Vec<u8>>(
                        namespace.to_owned().into(),
                        signatures.to_owned(),
                    )
                    .await?;
                Ok(BlobCommandOutput::Fetching(vec![blob]).into())
            }
            BlobSubCommand::Get {
                slot,
                lookback_slots,
            } => {
                let blobs = client
                    .get_ledger_blobs::<Vec<u8>>(
                        *slot,
                        namespace.to_owned().into(),
                        *lookback_slots,
                    )
                    .await?;
                Ok(BlobCommandOutput::Fetching(blobs).into())
            }
        }
    }
}
