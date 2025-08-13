use std::sync::Arc;

use anchor_lang::prelude::Pubkey;
use clap::Parser;
use data_anchor_api::BloberWithNamespace;
use data_anchor_blober::checkpoint::Checkpoint;
use data_anchor_client::{
    BloberIdentifier, DataAnchorClient, DataAnchorClientResult, FeeStrategy, Priority,
};
use data_anchor_utils::encoding::DataAnchorEncoding;
use serde::{Serialize, ser::SerializeStruct};
use tracing::{info, instrument};

use crate::{Cli, NAMESPACE_MISSING_MSG, formatting::CommandOutput};

#[derive(Debug, Clone, Parser, Serialize, PartialEq, Eq)]
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
    /// Query checkpoint status for the given blober account.
    #[command(visible_alias = "ch")]
    CheckpointStatus,
    /// Create an on-chain checkpoint for the given blober account.
    #[command(visible_alias = "cp")]
    ConfigureCheckpoint {
        /// The authority that can create the checkpoint for the given blober.
        #[arg(short, long)]
        authority: Pubkey,
    },
}

#[derive(Debug)]
pub struct BloberCommandOutput {
    identifier: BloberIdentifier,
    action: BloberSubCommand,
    program_id: Pubkey,
    payer: Pubkey,
    blobers: Vec<BloberWithNamespace>,
    checkpoint: Option<Checkpoint>,
}

impl Serialize for BloberCommandOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("BloberCommandOutput", 9)?;
        state.serialize_field(
            "identifier",
            &self
                .identifier
                .to_blober_address(self.program_id, self.payer)
                .to_string(),
        )?;
        state.serialize_field("action", &self.action)?;
        state.serialize_field("program_id", &self.program_id.to_string())?;
        state.serialize_field("payer", &self.payer.to_string())?;
        state.serialize_field("blobers", &self.blobers)?;
        let (proof, public_values, verification_key, slot) =
            if let Some(checkpoint) = &self.checkpoint {
                (
                    hex::encode(checkpoint.proof),
                    hex::encode(&checkpoint.public_values),
                    checkpoint.verification_key.as_str(),
                    checkpoint.slot,
                )
            } else {
                (String::new(), String::new(), "", 0)
            };
        state.serialize_field("checkpoint_proof", &proof)?;
        state.serialize_field("checkpoint_public_values", &public_values)?;
        state.serialize_field("checkpoint_verification_key", &verification_key)?;
        state.serialize_field("checkpoint_slot", &slot)?;
        state.end()
    }
}

impl std::fmt::Display for BloberCommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.action {
            BloberSubCommand::List => {
                write!(
                    f,
                    "Blober account addresses for program ID {}:\n{}",
                    self.payer,
                    self.blobers
                        .iter()
                        .map(|BloberWithNamespace { address, namespace }| format!(
                            "Pubkey: {}, Namespace: {namespace}",
                            address.0
                        ))
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
                    self.identifier
                        .to_blober_address(self.program_id, self.payer)
                )
            }
            BloberSubCommand::CheckpointStatus => {
                if let Some(checkpoint) = &self.checkpoint {
                    write!(
                        f,
                        "Checkpoint status for blober account {:?}:\nProof: {}\nPublic Values: {}\nVerification Key: {}\nSlot: {}",
                        self.identifier.namespace(),
                        hex::encode(checkpoint.proof),
                        hex::encode(&checkpoint.public_values),
                        checkpoint.verification_key,
                        checkpoint.slot
                    )
                } else {
                    write!(
                        f,
                        "No checkpoint found for blober account {:?}",
                        self.identifier.namespace()
                    )
                }
            }
            on_chain => {
                write!(
                    f,
                    "Blober account {:?} has been successfully {} (Pubkey: {})",
                    self.identifier.namespace(),
                    match on_chain {
                        BloberSubCommand::Initialize => "initialized".to_owned(),
                        BloberSubCommand::Close => "closed".to_owned(),
                        BloberSubCommand::ConfigureCheckpoint { authority } =>
                            format!("configured for checkpointing by {authority}"),
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
    pub async fn run<Encoding>(
        &self,
        client: Arc<DataAnchorClient<Encoding>>,
        identifier: BloberIdentifier,
        program_id: Pubkey,
        payer: Pubkey,
    ) -> DataAnchorClientResult<CommandOutput>
    where
        Encoding: DataAnchorEncoding,
    {
        let mut blobers = Vec::new();
        let mut checkpoint = None;
        match self {
            BloberSubCommand::Initialize => {
                let Some(namespace) = identifier.namespace() else {
                    Cli::exit_with_missing_arg(NAMESPACE_MISSING_MSG);
                };
                info!("Initializing blober account with namespace: {namespace}");
                client
                    .initialize_blober(
                        FeeStrategy::BasedOnRecentFees(Priority::Medium),
                        namespace.to_owned().into(),
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
                blobers = client.list_blobers().await?;
            }
            BloberSubCommand::CheckpointStatus => {
                info!(
                    "Querying checkpoint status for blober account with namespace: {}",
                    identifier.namespace().unwrap_or("unknown")
                );
                checkpoint = client.get_checkpoint(identifier.clone()).await?;
            }
            BloberSubCommand::ConfigureCheckpoint { authority } => {
                info!(
                    "Configuring checkpoint for blober account with namespace: {}",
                    identifier.namespace().unwrap_or("unknown")
                );
                client
                    .configure_checkpoint(
                        FeeStrategy::BasedOnRecentFees(Priority::Medium),
                        identifier.clone(),
                        *authority,
                        None,
                    )
                    .await?;
            }
        }
        Ok(BloberCommandOutput {
            identifier,
            action: self.clone(),
            program_id,
            payer,
            blobers,
            checkpoint,
        }
        .into())
    }
}
