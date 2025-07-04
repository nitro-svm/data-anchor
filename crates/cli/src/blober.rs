use std::sync::Arc;

use clap::Parser;
use data_anchor_api::{PubkeyFromStr, pubkey_with_str};
use data_anchor_blober::find_blober_address;
use data_anchor_client::{
    BloberIdentifier, DataAnchorClient, DataAnchorClientResult, FeeStrategy, Priority,
};
use serde::Serialize;
use solana_sdk::pubkey::Pubkey;
use tracing::{info, instrument};

use crate::{Cli, NAMESPACE_MISSING_MSG, formatting::CommandOutput};

#[derive(Debug, Clone, Copy, Parser, Serialize, PartialEq, Eq)]
pub enum BloberSubCommand {
    /// Initialize the given blober account.
    #[command(visible_alias = "i")]
    Initialize,
    /// Close the given blober account.
    #[command(visible_alias = "c")]
    Close,
    /// Get the address of the blober account for the given program ID and namespace.
    #[command(visible_alias = "a")]
    Address,
    /// Get all the PDA addresses for the given program ID.
    #[command(visible_alias = "l")]
    List,
}

#[derive(Debug, Serialize)]
pub struct BloberCommandOutput {
    identifier: BloberIdentifier,
    action: BloberSubCommand,
    #[serde(with = "pubkey_with_str")]
    program_id: Pubkey,
    #[serde(with = "pubkey_with_str")]
    payer: Pubkey,
    blobers: Vec<PubkeyFromStr>,
}

impl std::fmt::Display for BloberCommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.action {
            BloberSubCommand::List => {
                write!(
                    f,
                    "Blober account addresses for program ID {}:\n{}",
                    self.payer,
                    self.blobers
                        .iter()
                        .map(|pubkey| pubkey.0.to_string())
                        .collect::<Vec<String>>()
                        .join("\n")
                )
            }
            BloberSubCommand::Address => {
                let Some(namespace) = self.identifier.namespace() else {
                    return Err(std::fmt::Error);
                };
                write!(
                    f,
                    "Blober account address for namespace {namespace}: {}",
                    find_blober_address(self.program_id, self.payer, namespace)
                )
            }
            on_chain => {
                write!(
                    f,
                    "Blober account {:?} has been successfully {} (Pubkey: {})",
                    self.identifier.namespace(),
                    match on_chain {
                        BloberSubCommand::Initialize => "initialized",
                        BloberSubCommand::Close => "closed",
                        _ => unreachable!(),
                    },
                    self.identifier
                        .to_blober_address(self.program_id, self.payer)
                )
            }
        }
    }
}

impl BloberSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(
        &self,
        client: Arc<DataAnchorClient>,
        identifier: BloberIdentifier,
        program_id: Pubkey,
        payer: Pubkey,
    ) -> DataAnchorClientResult<CommandOutput> {
        let mut blobers = Vec::new();
        match self {
            BloberSubCommand::Initialize => {
                let Some(namespace) = identifier.namespace() else {
                    Cli::exit_with_missing_arg(NAMESPACE_MISSING_MSG);
                };
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
                        identifier.clone(),
                        None,
                    )
                    .await?;
            }
            BloberSubCommand::Address => {
                // No action needed for address command, just return the output.
            }
            BloberSubCommand::List => {
                blobers = client
                    .list_blobers()
                    .await?
                    .into_iter()
                    .map(PubkeyFromStr)
                    .collect();
            }
        }
        Ok(BloberCommandOutput {
            identifier,
            action: *self,
            program_id,
            payer,
            blobers,
        }
        .into())
    }
}
