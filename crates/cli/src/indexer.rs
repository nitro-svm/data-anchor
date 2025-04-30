use std::sync::Arc;

use clap::{Args, Parser};
use itertools::Itertools;
use nitro_da_client::{BloberClient, BloberClientResult};
use nitro_da_indexer_api::CompoundProof;
use serde::Serialize;
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
    Proofs(Box<CompoundProof>),
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
                Ok(IndexerCommandOutput::Proofs(Box::new(proof)).into())
            }
        }
    }
}
