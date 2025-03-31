use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use nitro_da_client::{BloberClient, BloberClientResult, FeeStrategy, Priority, TransactionType};
use serde::Serialize;
use solana_sdk::{clock::Slot, pubkey::Pubkey, signature::Signature};
use tracing::instrument;

use crate::formatting::CommandOutput;

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

#[derive(Debug, Serialize)]
pub struct BlobCommandOutput {
    slot: Slot,
    signatures: Vec<Signature>,
    success: bool,
}

impl std::fmt::Display for BlobCommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Slot: {}, Signatures: [{}], Success: {}",
            self.slot,
            self.signatures
                .iter()
                .map(|sig| sig.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            self.success
        )
    }
}

impl BlobSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(
        &self,
        client: Arc<BloberClient>,
        blober: Pubkey,
    ) -> BloberClientResult<CommandOutput> {
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
                let last_tx = results.last().expect("there should be at least one result");
                Ok(BlobCommandOutput {
                    slot: last_tx.slot,
                    signatures: results.iter().map(|tx| tx.signature).collect(),
                    success: matches!(last_tx.data, TransactionType::FinalizeBlob),
                }
                .into())
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
                let last_tx = results.last().expect("there should be at least one result");
                Ok(BlobCommandOutput {
                    slot: last_tx.slot,
                    signatures: results.iter().map(|tx| tx.signature).collect(),
                    success: matches!(last_tx.data, TransactionType::DiscardBlob),
                }
                .into())
            }
        }
    }
}
