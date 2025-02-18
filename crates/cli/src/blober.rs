use std::sync::Arc;

use clap::Parser;
use nitro_da_client::{BloberClient, BloberClientResult, FeeStrategy, Priority};
use solana_sdk::pubkey::Pubkey;
use tracing::instrument;

#[derive(Debug, Parser)]
pub enum BloberSubCommand {
    /// Initialize the given blober account.
    #[command(visible_alias = "i")]
    Initialize,
    /// Close the given blober account.
    #[command(visible_alias = "c")]
    Close,
}

impl BloberSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(&self, client: Arc<BloberClient>, blober: Pubkey) -> BloberClientResult {
        match self {
            BloberSubCommand::Initialize => {
                client
                    .initialize_blober(
                        FeeStrategy::BasedOnRecentFees(Priority::Medium),
                        blober,
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
        Ok(())
    }
}
