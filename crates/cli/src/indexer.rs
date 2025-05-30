use std::sync::Arc;

use chrono::{DateTime, Utc};
use clap::{Args, Parser};
use itertools::Itertools;
use nitro_da_client::{BloberClient, BloberClientResult};
use nitro_da_indexer_api::{BlobsByBlober, BlobsByPayer, CompoundProof, TimeRange};
use serde::Serialize;
use solana_sdk::pubkey::Pubkey;
use tracing::instrument;

use crate::formatting::CommandOutput;

#[derive(Debug, Parser)]
pub enum IndexerSubCommand {
    /// Get blobs for a given slot.
    #[command(visible_alias = "b")]
    Blobs(SlotArgs),
    /// Get compound proof for a given slot.
    #[command(visible_alias = "p")]
    Proofs(SlotArgs),
    /// Get blobs for a given blober.
    #[command(visible_alias = "bl")]
    BlobsForBlober {
        /// The blober address to query.
        #[arg(short, long)]
        blober: Pubkey,
        #[clap(flatten)]
        time_args: TimeArgs,
    },
    /// Get blobs for a given payer.
    #[command(visible_alias = "bp")]
    BlobsForPayer {
        /// The payer address to query.
        #[arg(short = 'y', long)]
        payer: Pubkey,
        /// The network name to query.
        #[arg(short = 'm', long)]
        network_name: String,
        #[clap(flatten)]
        time_args: TimeArgs,
    },
    /// Get proof for a given blob.
    #[command(visible_alias = "pb")]
    ProofForBlob {
        /// The blob address to query.
        #[arg(short, long)]
        blob: Pubkey,
    },
}

#[derive(Debug, Clone, Args)]
pub struct TimeArgs {
    /// The start time to query from.
    #[arg(long, value_parser=DateTime::parse_from_rfc3339)]
    pub start: Option<DateTime<Utc>>,
    /// The end time to query until.
    #[arg(long, value_parser=DateTime::parse_from_rfc3339)]
    pub end: Option<DateTime<Utc>>,
}

#[derive(Debug, Args)]
pub struct SlotArgs {
    /// The slot to query.
    pub slot: u64,
}

#[derive(Debug, Serialize)]
pub enum IndexerCommandOutput {
    /// The blobs for the given slot.
    Blobs(Vec<Vec<u8>>),
    /// The compound proof for the given slot.
    Proofs(Box<Option<CompoundProof>>),
}

impl std::fmt::Display for IndexerCommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexerCommandOutput::Blobs(blobs) => {
                write!(
                    f,
                    "Blobs: [{}]",
                    blobs.iter().map(hex::encode).collect_vec().join(", ")
                )
            }
            IndexerCommandOutput::Proofs(proof) => {
                write!(f, "Proofs: {proof:?}")
            }
        }
    }
}

impl IndexerSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(
        &self,
        client: Arc<BloberClient>,
        namespace: &str,
    ) -> BloberClientResult<CommandOutput> {
        match self {
            IndexerSubCommand::Blobs(SlotArgs { slot }) => {
                let data = client.get_blobs(*slot, namespace, None).await?;
                Ok(IndexerCommandOutput::Blobs(data).into())
            }
            IndexerSubCommand::Proofs(SlotArgs { slot }) => {
                let proof = client.get_slot_proof(*slot, namespace, None).await?;
                Ok(IndexerCommandOutput::Proofs(Box::new(Some(proof))).into())
            }
            IndexerSubCommand::BlobsForBlober {
                blober,
                time_args: TimeArgs { start, end },
            } => {
                let data = client
                    .get_blobs_by_blober(BlobsByBlober {
                        blober: blober.to_owned(),
                        time_range: TimeRange {
                            start: start.to_owned(),
                            end: end.to_owned(),
                        },
                    })
                    .await?;
                Ok(IndexerCommandOutput::Blobs(data).into())
            }
            IndexerSubCommand::BlobsForPayer {
                payer,
                network_name,
                time_args: TimeArgs { start, end },
            } => {
                let data = client
                    .get_blobs_by_payer(BlobsByPayer {
                        payer: payer.to_owned(),
                        network_name: network_name.to_owned(),
                        time_range: TimeRange {
                            start: start.to_owned(),
                            end: end.to_owned(),
                        },
                    })
                    .await?;
                Ok(IndexerCommandOutput::Blobs(data).into())
            }
            IndexerSubCommand::ProofForBlob { blob } => {
                let proof = client.get_blob_proof(blob.to_owned()).await?;
                Ok(IndexerCommandOutput::Proofs(Box::new(proof)).into())
            }
        }
    }
}
