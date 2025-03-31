use std::sync::Arc;

use clap::Parser;
use nitro_da_client::{BloberClient, BloberClientResult, FeeStrategy, Priority};
use serde::Serialize;
use solana_sdk::pubkey::Pubkey;
use tracing::{info, instrument};

use crate::formatting::CommandOutput;

#[derive(Debug, Clone, Copy, Parser, Serialize)]
pub enum BloberSubCommand {
    /// Initialize the given blober account.
    #[command(visible_alias = "i")]
    Initialize,
    /// Close the given blober account.
    #[command(visible_alias = "c")]
    Close,
}

#[derive(Debug, Serialize)]
pub struct BloberCommandOutput {
    address: Pubkey,
    action: BloberSubCommand,
}

impl std::fmt::Display for BloberCommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Blober account {} has been successfully {}",
            self.address,
            match self.action {
                BloberSubCommand::Initialize => "initialized",
                BloberSubCommand::Close => "closed",
            }
        )
    }
}

impl BloberSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(
        &self,
        client: Arc<BloberClient>,
        blober: Pubkey,
        namespace: String,
    ) -> BloberClientResult<CommandOutput> {
        match self {
            BloberSubCommand::Initialize => {
                info!("Initializing blober account with address: {blober}");
                client
                    .initialize_blober(
                        FeeStrategy::BasedOnRecentFees(Priority::Medium),
                        namespace,
                        None,
                    )
                    .await?;
            }
            BloberSubCommand::Close => {
                client
                    .close_blober(
                        FeeStrategy::BasedOnRecentFees(Priority::Medium),
                        blober,
                        None,
                    )
                    .await?;
            }
        }
        Ok(BloberCommandOutput {
            address: blober,
            action: *self,
        }
        .into())
    }
}
