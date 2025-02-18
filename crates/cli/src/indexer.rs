use std::sync::Arc;

use clap::{Args, Parser};
use nitro_da_client::{BloberClient, BloberClientResult};
use solana_sdk::pubkey::Pubkey;
use tracing::instrument;

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

impl IndexerSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(&self, client: Arc<BloberClient>, blober: Pubkey) -> BloberClientResult {
        match self {
            IndexerSubCommand::Blobs(SlotArgs { slot }) => {
                client.get_blobs(*slot, blober).await?;
            }
            IndexerSubCommand::Proofs(SlotArgs { slot }) => {
                client.get_slot_proof(*slot, blober).await?;
            }
        }
        Ok(())
    }
}
