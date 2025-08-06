use std::sync::Arc;

use anchor_lang::prelude::Pubkey;
use chrono::{DateTime, Utc};
use clap::{Args, Parser};
use data_anchor_api::{CompoundInclusionProof, CustomerElf, TimeRange};
use data_anchor_client::{DataAnchorClient, DataAnchorClientResult};
use itertools::Itertools;
use serde::Serialize;
use tracing::instrument;

use crate::formatting::CommandOutput;

#[derive(Debug, Parser)]
pub enum IndexerSubCommand {
    /// Get blobs for a given slot.
    #[command(visible_alias = "b")]
    Blobs(SlotArgs),
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
        blob_payer: Pubkey,
        /// The network name to query.
        #[arg(short = 'm', long)]
        network_name: String,
        #[clap(flatten)]
        time_args: TimeArgs,
    },
    /// Get blobs for a given network and time range.
    #[command(visible_alias = "bn")]
    BlobsForNetwork {
        /// The network name to query.
        #[arg(short = 'm', long)]
        network_name: String,
        #[clap(flatten)]
        time_args: TimeArgs,
    },
    /// Get blobs for a given namespace and time range.
    #[command(visible_alias = "ns")]
    BlobsForNamespace {
        /// The namespace to query.
        #[arg(short, long)]
        namespace: String,
        /// The payer address to query.
        #[arg(long)]
        payer_pubkey: Option<Pubkey>,
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
    /// Get compound proof for a given slot.
    #[command(visible_alias = "p", alias = "proofs")]
    Proof(SlotArgs),
    /// Request a custom proof for a given slot.
    #[command(visible_alias = "zkp")]
    ZKProof {
        /// The slot to query.
        #[arg(short, long)]
        slot: u64,
        /// The proof type to request.
        #[arg(long, value_enum)]
        proof_type: CustomerElf,
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
    Proofs(Box<Option<CompoundInclusionProof>>),
    /// The request ID for the ZK proof generation.
    ZKProofs(String),
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
            IndexerCommandOutput::ZKProofs(proof) => {
                write!(f, "ZK Proofs: {proof:?}")
            }
        }
    }
}

impl IndexerSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(
        &self,
        client: Arc<DataAnchorClient>,
        blober_pda: Pubkey,
    ) -> DataAnchorClientResult<CommandOutput> {
        match self {
            IndexerSubCommand::Blobs(SlotArgs { slot }) => {
                let data = client.get_blobs(*slot, blober_pda.into()).await?;
                Ok(IndexerCommandOutput::Blobs(data.unwrap_or_default()).into())
            }
            IndexerSubCommand::BlobsForBlober {
                blober,
                time_args: TimeArgs { start, end },
            } => {
                let data = client
                    .get_blobs_by_blober(
                        (*blober).into(),
                        Some(TimeRange {
                            start: start.to_owned(),
                            end: end.to_owned(),
                        }),
                    )
                    .await?;
                Ok(IndexerCommandOutput::Blobs(data).into())
            }
            IndexerSubCommand::BlobsForPayer {
                blob_payer,
                network_name,
                time_args: TimeArgs { start, end },
            } => {
                let data = client
                    .get_blobs_by_payer(
                        blob_payer.to_owned(),
                        network_name.to_owned(),
                        Some(TimeRange {
                            start: start.to_owned(),
                            end: end.to_owned(),
                        }),
                    )
                    .await?;
                Ok(IndexerCommandOutput::Blobs(data).into())
            }
            IndexerSubCommand::BlobsForNetwork {
                network_name,
                time_args: TimeArgs { start, end },
            } => {
                let data = client
                    .get_blobs_by_network(
                        network_name.to_owned(),
                        Some(TimeRange {
                            start: start.to_owned(),
                            end: end.to_owned(),
                        }),
                    )
                    .await?;
                Ok(IndexerCommandOutput::Blobs(data).into())
            }
            IndexerSubCommand::BlobsForNamespace {
                namespace,
                payer_pubkey,
                time_args: TimeArgs { start, end },
            } => {
                let data = client
                    .get_blobs_by_namespace_for_payer(
                        namespace.to_owned(),
                        payer_pubkey.to_owned(),
                        Some(TimeRange {
                            start: start.to_owned(),
                            end: end.to_owned(),
                        }),
                    )
                    .await?;
                Ok(IndexerCommandOutput::Blobs(data).into())
            }
            IndexerSubCommand::Proof(SlotArgs { slot }) => {
                let proof = client.get_proof(*slot, blober_pda.into()).await?;
                Ok(IndexerCommandOutput::Proofs(Box::new(proof)).into())
            }
            IndexerSubCommand::ProofForBlob { blob } => {
                let proof = client.get_proof_for_blob(blob.to_owned()).await?;
                Ok(IndexerCommandOutput::Proofs(Box::new(proof)).into())
            }
            IndexerSubCommand::ZKProof { slot, proof_type } => {
                let request_id = client
                    .checkpoint_custom_proof(*slot, blober_pda.into(), *proof_type)
                    .await?;
                Ok(IndexerCommandOutput::ZKProofs(request_id).into())
            }
        }
    }
}
