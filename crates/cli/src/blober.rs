use std::sync::Arc;

use clap::Parser;
use data_anchor_blober::find_blober_address;
use data_anchor_client::{DataAnchorClient, DataAnchorClientResult, FeeStrategy, Priority};
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
    namespace: String,
    action: BloberSubCommand,
    program_id: Pubkey,
    payer: Pubkey,
}

impl std::fmt::Display for BloberCommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Blober account {} has been successfully {} (Pubkey: {})",
            self.namespace,
            match self.action {
                BloberSubCommand::Initialize => "initialized",
                BloberSubCommand::Close => "closed",
            },
            find_blober_address(self.program_id, self.payer, &self.namespace)
        )
    }
}

impl BloberSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(
        &self,
        client: Arc<DataAnchorClient>,
        namespace: &str,
        program_id: Pubkey,
        payer: Pubkey,
    ) -> DataAnchorClientResult<CommandOutput> {
        match self {
            BloberSubCommand::Initialize => {
                info!("Initializing blober account with namespace: {namespace}");
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
                        namespace,
                        None,
                    )
                    .await?;
            }
        }
        Ok(BloberCommandOutput {
            namespace: namespace.to_owned(),
            action: *self,
            program_id,
            payer,
        }
        .into())
    }
}
