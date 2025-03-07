use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use nitro_da_client::{BloberClient, BloberClientResult, FeeStrategy, Priority, TransactionType};
use solana_sdk::pubkey::Pubkey;
use tracing::instrument;

#[derive(Debug, Parser)]
pub enum BlobSubCommand {
    /// Upload a blob of data.
    #[command(visible_alias = "u")]
    Upload {
        /// The path to the data to upload.
        data_path: PathBuf,
    },
    /// Discard a blob.
    #[command(visible_alias = "d")]
    Discard {
        /// The Pubkey of the blob to discard.
        blob: Pubkey,
    },
}

impl BlobSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(&self, client: Arc<BloberClient>, blober: Pubkey) -> BloberClientResult {
        match self {
            BlobSubCommand::Upload { data_path } => {
                let blob_data = tokio::fs::read(data_path).await.unwrap();
                let results = client
                    .upload_blob(
                        &blob_data,
                        FeeStrategy::BasedOnRecentFees(Priority::VeryHigh),
                        blober,
                        None,
                    )
                    .await?;
                let finalize_tx = results.last().expect("there should be at least one result");
                match finalize_tx.data {
                    TransactionType::DiscardBlob => {
                        eprintln!("Blob upload failed, blob discarded.")
                    }
                    TransactionType::FinalizeBlob => println!(
                        "Blob uploaded successfully at slot {} with signature {}",
                        finalize_tx.slot, finalize_tx.signature
                    ),
                    _ => unreachable!("unexpected transaction type"),
                }
            }
            BlobSubCommand::Discard { blob } => {
                let results = client
                    .discard_blob(
                        FeeStrategy::BasedOnRecentFees(Priority::VeryHigh),
                        *blob,
                        blober,
                        None,
                    )
                    .await?;
                let finalize_tx = results.last().expect("there should be at least one result");
                match finalize_tx.data {
                    TransactionType::DiscardBlob => {
                        println!("Blob discarded successfully.")
                    }
                    _ => unreachable!("unexpected transaction type"),
                }
            }
        }
        Ok(())
    }
}
