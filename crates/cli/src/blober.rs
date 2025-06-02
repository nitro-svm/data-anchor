use std::sync::Arc;

use clap::Parser;
use data_anchor_client::{BloberClient, BloberClientResult, FeeStrategy, Priority};
use serde::Serialize;
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
}

impl std::fmt::Display for BloberCommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Blober account {} has been successfully {}",
            self.namespace,
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
        namespace: &str,
    ) -> BloberClientResult<CommandOutput> {
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
        }
        .into())
    }
}
