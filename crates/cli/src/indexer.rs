use std::sync::Arc;

use anchor_lang::prelude::Pubkey;
use chrono::{DateTime, Utc};
use clap::{Args, Parser};
use data_anchor_api::{CompoundInclusionProof, CustomerElf, RequestStatus, TimeRange};
use data_anchor_client::{DataAnchorClient, DataAnchorClientResult};
use data_anchor_utils::{compression::DataAnchorCompression, encoding::DataAnchorEncoding};
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
    /// List payers for a given network.
    #[command(visible_alias = "lp")]
    ListPayers {
        /// The network name to query.
        #[arg(short = 'm', long)]
        network_name: String,
    },
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
    /// Get the status of a proof request.
    #[command(visible_alias = "prs")]
    ProofRequestStatus {
        /// The request ID of the proof request.
        #[arg(short, long)]
        request_id: String,
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
    /// The status of a proof request.
    ProofRequestStatus(String, RequestStatus),
    /// The list of payers for a given network.
    Payers(Vec<Pubkey>),
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
            IndexerCommandOutput::ProofRequestStatus(request_id, status) => {
                write!(
                    f,
                    "Proof Request Status: Request ID: {request_id}, Status: {status:?}"
                )
            }
            IndexerCommandOutput::Payers(payers) => {
                write!(
                    f,
                    "Payers: [{}]",
                    payers
                        .iter()
                        .map(|p| p.to_string())
                        .collect_vec()
                        .join(", ")
                )
            }
        }
    }
}

impl IndexerSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run<Encoding, Compression>(
        &self,
        client: Arc<DataAnchorClient<Encoding, Compression>>,
        blober_pda: Pubkey,
    ) -> DataAnchorClientResult<CommandOutput>
    where
        Encoding: DataAnchorEncoding,
        Compression: DataAnchorCompression,
    {
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
            IndexerSubCommand::ListPayers { network_name } => {
                let payers = client
                    .get_payers_by_network(network_name.to_owned())
                    .await?;
                Ok(IndexerCommandOutput::Payers(payers.into_iter().map_into().collect()).into())
            }
            IndexerSubCommand::ZKProof { slot, proof_type } => {
                let request_id = client
                    .checkpoint_custom_proof(*slot, blober_pda.into(), *proof_type)
                    .await?;
                Ok(IndexerCommandOutput::ZKProofs(request_id).into())
            }
            IndexerSubCommand::ProofRequestStatus { request_id } => {
                let status = client
                    .get_proof_request_status(request_id.to_owned())
                    .await?;
                Ok(IndexerCommandOutput::ProofRequestStatus(request_id.to_owned(), status).into())
            }
        }
    }
}
