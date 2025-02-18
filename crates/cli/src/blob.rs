use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use nitro_da_client::{BloberClient, BloberClientResult, FeeStrategy, Priority};
use solana_sdk::pubkey::Pubkey;
use tracing::instrument;

#[derive(Debug, Parser)]
pub enum BlobSubCommand {
    /// Upload a blob of data.
    #[command(visible_alias = "u")]
    Upload {
        /// The path to the data to upload.
        #[arg(short, long)]
        data_path: PathBuf,
    },
    /// Discard a blob.
    #[command(visible_alias = "d")]
    Discard {
        /// The Pubkey of the blob to discard.
        #[arg(short, long)]
        blob: Pubkey,
    },
}

impl BlobSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(&self, client: Arc<BloberClient>, blober: Pubkey) -> BloberClientResult {
        match self {
            BlobSubCommand::Upload { data_path } => {
                let blob_data = tokio::fs::read(data_path).await.unwrap();
                client
                    .upload_blob(
                        &blob_data,
                        FeeStrategy::BasedOnRecentFees(Priority::Medium),
                        blober,
                        None,
                    )
                    .await?;
            }
            BlobSubCommand::Discard { blob } => {
                client
                    .discard_blob(
                        FeeStrategy::BasedOnRecentFees(Priority::Medium),
                        *blob,
                        blober,
                        None,
                    )
                    .await?;
            }
        }
        Ok(())
    }
}
