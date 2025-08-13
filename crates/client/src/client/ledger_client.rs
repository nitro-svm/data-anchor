use std::collections::{HashMap, HashSet};

use anchor_lang::{
    AnchorDeserialize, Discriminator, prelude::Pubkey, solana_program::message::VersionedMessage,
};
use data_anchor_api::{
    BloberWithNamespace, LedgerDataBlobError, RelevantInstruction, RelevantInstructionWithAccounts,
    extract_relevant_instructions, get_account_at_index, get_blob_data_from_instructions,
};
use data_anchor_blober::{
    BLOB_ACCOUNT_INSTRUCTION_IDX, BLOB_BLOBER_INSTRUCTION_IDX, checkpoint::Checkpoint,
    find_checkpoint_address, state::blober::Blober,
};
use data_anchor_utils::{
    compression::DataAnchorCompression,
    encoding::{DataAnchorEncoding, Decodable},
};
use futures::{StreamExt, TryStreamExt};
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_client::{
    rpc_config::{
        RpcAccountInfoConfig, RpcBlockConfig, RpcProgramAccountsConfig, RpcTransactionConfig,
    },
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_rpc_client_api::client_error::Error;
use solana_signature::Signature;
use solana_signer::Signer;
use solana_transaction_status::{EncodedConfirmedBlock, UiTransactionEncoding};

use super::BloberIdentifier;
use crate::{
    DataAnchorClient, DataAnchorClientResult, OutcomeError,
    constants::{DEFAULT_CONCURRENCY, DEFAULT_LOOKBACK_SLOTS},
    helpers::filter_relevant_instructions,
};

/// An error that can occur when uploading a blob to a blober account.
#[derive(thiserror::Error, Debug)]
pub enum ChainError {
    /// Failed to query Solana RPC: {0}
    #[error("Failed to query Solana RPC: {0}")]
    SolanaRpc(#[from] Error),
    /// Failed when sending transactions. Transaction errors:\n{}
    #[error(transparent)]
    TransactionFailure(#[from] OutcomeError),
    /// Fee Strategy conversion failure: {0}
    #[error("Fee Strategy conversion failure: {0}")]
    ConversionError(&'static str),
    /// Failed to declare blob: {0}
    #[error("Failed to declare blob: {0}")]
    DeclareBlob(OutcomeError),
    /// Failed to insert chunks: {0}
    #[error("Failed to insert chunks: {0}")]
    InsertChunks(OutcomeError),
    /// Failed to finalize blob: {0}
    #[error("Failed to finalize blob: {0}")]
    FinalizeBlob(OutcomeError),
    /// Failed to discard blob: {0}
    #[error("Failed to discard blob: {0}")]
    DiscardBlob(OutcomeError),
    /// Failed to compound upload: {0}
    #[error("Failed to compound upload: {0}")]
    CompoundUpload(OutcomeError),
    /// Failed to initialize blober: {0}
    #[error("Failed to initialize blober: {0}")]
    InitializeBlober(OutcomeError),
    /// Failed to close blober: {0}
    #[error("Failed to close blober: {0}")]
    CloseBlober(OutcomeError),
    /// Missing blober namespace
    #[error("Missing blober namespace. Namespace is required for creating a blober account.")]
    MissingBloberNamespace,
    /// Account already exists: {0}
    #[error("Account already exists: {0}")]
    AccountExists(String),
    /// Account does not exist: {0}
    #[error("Account does not exist: {0}")]
    AccountDoesNotExist(String),
    /// Payer has insufficient balance to pay for the transaction: required {0} lamports, available {1} lamports
    #[error(
        "Payer has insufficient balance to pay for the transaction: required {0} lamports, available {1} lamports"
    )]
    InsufficientBalance(u64, u64),
    /// Could not calculate cost
    #[error("Could not calculate cost")]
    CouldNotCalculateCost,
    /// Failed to configure checkpoint: {0}
    #[error("Failed to configure checkpoint: {0}")]
    ConfigureCheckpoint(OutcomeError),
    /// Provided proof commitment does not match the blober's address
    #[error("Provided proof commitment does not match the blober's address expected {0}, got {1}")]
    ProofBloberMismatch(Pubkey, Pubkey),
    #[error("Checkpoint account is not up to date with current blober state")]
    CheckpointNotUpToDate,
}

impl<Encoding, Compression> DataAnchorClient<Encoding, Compression>
where
    Encoding: DataAnchorEncoding,
    Compression: DataAnchorCompression,
{
    /// Returns the raw blob data from the ledger for the given signatures.
    pub async fn get_ledger_blobs_from_signatures<T>(
        &self,
        identifier: BloberIdentifier,
        signatures: Vec<Signature>,
    ) -> DataAnchorClientResult<T>
    where
        T: Decodable,
    {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        let relevant_transactions = futures::stream::iter(signatures)
            .map(|signature| async move {
                self.rpc_client
                    .get_transaction_with_config(
                        &signature,
                        RpcTransactionConfig {
                            commitment: Some(self.rpc_client.commitment()),
                            encoding: Some(UiTransactionEncoding::Base58),
                            max_supported_transaction_version: Some(0),
                        },
                    )
                    .await
            })
            .buffer_unordered(DEFAULT_CONCURRENCY)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        let relevant_instructions = extract_relevant_instructions(
            &self.program_id,
            &relevant_transactions
                .iter()
                .filter_map(|encoded| match &encoded.transaction.meta {
                    Some(meta) if meta.status.is_err() => None,
                    _ => encoded.transaction.transaction.decode(),
                })
                .collect::<Vec<_>>(),
        );

        let declares = relevant_instructions
            .iter()
            .filter_map(|instruction| {
                (instruction.blober == blober
                    && matches!(instruction.instruction, RelevantInstruction::DeclareBlob(_)))
                .then_some(instruction.blob)
            })
            .collect::<Vec<Pubkey>>();

        let Some(blob) = declares.first() else {
            return Err(LedgerDataBlobError::DeclareNotFound.into());
        };

        if declares.len() > 1 {
            return Err(LedgerDataBlobError::MultipleDeclares.into());
        }

        if relevant_instructions
            .iter()
            .filter(|instruction| {
                matches!(
                    instruction.instruction,
                    RelevantInstruction::FinalizeBlob(_)
                )
            })
            .count()
            > 1
        {
            return Err(LedgerDataBlobError::MultipleFinalizes.into());
        }

        let data = get_blob_data_from_instructions(&relevant_instructions, blober, *blob)?;

        self.decompress_and_decode(&data).await
    }

    /// Fetches all blobs finalized in a given slot from the ledger.
    pub async fn get_ledger_blobs<T>(
        &self,
        slot: u64,
        identifier: BloberIdentifier,
        lookback_slots: Option<u64>,
    ) -> DataAnchorClientResult<Vec<T>>
    where
        T: Decodable,
    {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        let block_config = RpcBlockConfig {
            commitment: Some(self.rpc_client.commitment()),
            encoding: Some(UiTransactionEncoding::Base58),
            max_supported_transaction_version: Some(0),
            ..Default::default()
        };
        let block = self
            .rpc_client
            .get_block_with_config(slot, block_config)
            .await?;

        let Some(transactions) = block.transactions else {
            // If there are no transactions in the block, that means there are no blobs to fetch.
            return Ok(Vec::new());
        };

        let relevant_instructions = extract_relevant_instructions(
            &self.program_id,
            &transactions
                .iter()
                .filter_map(|tx| match &tx.meta {
                    Some(meta) if meta.status.is_err() => None,
                    _ => tx.transaction.decode(),
                })
                .collect::<Vec<_>>(),
        );
        let finalized_blobs = relevant_instructions
            .iter()
            .filter_map(|instruction| {
                (instruction.blober == blober
                    && matches!(
                        instruction.instruction,
                        RelevantInstruction::FinalizeBlob(_)
                    ))
                .then_some(instruction.blob)
            })
            .collect::<HashSet<Pubkey>>();

        let mut relevant_instructions_map = HashMap::new();
        filter_relevant_instructions(
            relevant_instructions,
            &finalized_blobs,
            &mut relevant_instructions_map,
        );

        let mut blobs = HashMap::with_capacity(finalized_blobs.len());
        for blob in &finalized_blobs {
            let instructions = relevant_instructions_map
                .get(blob)
                .expect("This should never happen since we at least have the finalize instruction");

            if let Ok(blob_data) = get_blob_data_from_instructions(instructions, blober, *blob) {
                blobs.insert(blob, blob_data);
            }
        }

        // If all blobs are found, return them.
        if blobs.len() == finalized_blobs.len() {
            let blob_data = futures::stream::iter(blobs.values())
                .map(|data| async move { self.decompress_and_decode(data).await })
                .buffer_unordered(DEFAULT_CONCURRENCY)
                .try_collect()
                .await?;

            return Ok(blob_data);
        }

        let lookback_slots = lookback_slots.unwrap_or(DEFAULT_LOOKBACK_SLOTS);

        let block_slots = self
            .rpc_client
            .get_blocks_with_commitment(
                slot - lookback_slots,
                Some(slot - 1),
                self.rpc_client.commitment(),
            )
            .await?;

        for slot in block_slots.into_iter().rev() {
            let block = self
                .rpc_client
                .get_block_with_config(slot, block_config)
                .await?;
            let Some(transactions) = block.transactions else {
                // If there are no transactions in the block, go to the next block.
                continue;
            };
            let new_relevant_instructions = extract_relevant_instructions(
                &self.program_id,
                &transactions
                    .iter()
                    .filter_map(|tx| match &tx.meta {
                        Some(meta) if meta.status.is_err() => None,
                        _ => tx.transaction.decode(),
                    })
                    .collect::<Vec<_>>(),
            );
            filter_relevant_instructions(
                new_relevant_instructions,
                &finalized_blobs,
                &mut relevant_instructions_map,
            );
            for blob in &finalized_blobs {
                if blobs.contains_key(blob) {
                    continue;
                }
                let instructions = relevant_instructions_map.get(blob).expect(
                    "This should never happen since we at least have the finalize instruction",
                );

                if let Ok(blob_data) = get_blob_data_from_instructions(instructions, blober, *blob)
                {
                    blobs.insert(blob, blob_data);
                }
            }
            if blobs.len() == finalized_blobs.len() {
                break;
            }
        }

        let blob_data = futures::stream::iter(blobs.values())
            .map(|data| async move { self.decompress_and_decode(data).await })
            .buffer_unordered(DEFAULT_CONCURRENCY)
            .try_collect()
            .await?;

        Ok(blob_data)
    }

    /// Fetches blob messages for a given slot
    /// Returns a tuple of ([`Pubkey`], [`VersionedMessage`]) where the Pubkey is the address of
    /// the [`data_anchor_blober::state::blob::Blob`] account and the VersionedMessage is the message
    /// that included the [`data_anchor_blober::instruction::FinalizeBlob`] instruction.
    pub async fn get_blob_messages(
        &self,
        slot: u64,
        identifier: BloberIdentifier,
    ) -> DataAnchorClientResult<Vec<(Pubkey, VersionedMessage)>> {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        let block: EncodedConfirmedBlock = self
            .rpc_client
            .get_block_with_config(
                slot,
                RpcBlockConfig {
                    commitment: Some(self.rpc_client.commitment()),
                    encoding: Some(UiTransactionEncoding::Base58),
                    max_supported_transaction_version: Some(0),
                    ..Default::default()
                },
            )
            .await?
            .into();

        let finalized = block
            .transactions
            .iter()
            .filter_map(|tx| match &tx.meta {
                Some(meta) if meta.status.is_err() => None,
                _ => tx.transaction.decode(),
            })
            .filter_map(|tx| {
                let instructions = tx
                    .message
                    .instructions()
                    .iter()
                    .filter_map(|compiled_instruction| {
                        Some(RelevantInstructionWithAccounts {
                            blob: get_account_at_index(
                                &tx,
                                compiled_instruction,
                                BLOB_ACCOUNT_INSTRUCTION_IDX,
                            )?,
                            blober: get_account_at_index(
                                &tx,
                                compiled_instruction,
                                BLOB_BLOBER_INSTRUCTION_IDX,
                            )?,
                            instruction: RelevantInstruction::try_from_slice(compiled_instruction)?,
                        })
                    })
                    .filter(|instruction| {
                        instruction.blober == blober
                            && matches!(
                                instruction.instruction,
                                RelevantInstruction::FinalizeBlob(_)
                            )
                    })
                    .collect::<Vec<_>>();

                instructions.is_empty().then_some(
                    instructions
                        .iter()
                        .map(|instruction| (instruction.blob, tx.message.clone()))
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect::<Vec<_>>();

        Ok(finalized)
    }

    /// Lists all blober accounts owned by the payer.
    pub async fn list_blobers(&self) -> DataAnchorClientResult<Vec<BloberWithNamespace>> {
        let blobers = self
            .rpc_client
            .get_program_accounts_with_config(
                &self.program_id,
                RpcProgramAccountsConfig {
                    filters: Some(vec![RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                        0,
                        Blober::DISCRIMINATOR.to_vec(),
                    ))]),
                    account_config: RpcAccountInfoConfig {
                        encoding: Some(UiAccountEncoding::Base64),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .await?;

        Ok(blobers
            .into_iter()
            .filter_map(|(pubkey, account)| {
                let state = account.data.get(Blober::DISCRIMINATOR.len()..)?;
                let blober_state = Blober::try_from_slice(state).ok()?;

                (blober_state.caller == self.payer.pubkey()).then_some(BloberWithNamespace {
                    address: pubkey.into(),
                    namespace: blober_state.namespace,
                })
            })
            .collect())
    }

    /// Retrieves a blober account by its identifier.
    pub async fn get_blober(
        &self,
        identifier: BloberIdentifier,
    ) -> DataAnchorClientResult<Option<Blober>> {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());
        let account = self
            .rpc_client
            .get_account_with_commitment(&blober, self.rpc_client.commitment())
            .await?
            .value;

        let Some(account) = account else {
            return Ok(None);
        };

        if !account.data.starts_with(Blober::DISCRIMINATOR) {
            return Err(LedgerDataBlobError::InvalidBloberAccount(
                "Invalid discriminator".to_owned(),
            )
            .into());
        }

        let mut state = account.data.get(Blober::DISCRIMINATOR.len()..).ok_or(
            LedgerDataBlobError::InvalidBloberAccount("No state data".to_owned()),
        )?;

        if state.is_empty() {
            return Err(
                LedgerDataBlobError::InvalidBloberAccount("Empty state data".to_owned()).into(),
            );
        }

        let blober = Blober::deserialize(&mut state).map_err(|e| {
            LedgerDataBlobError::InvalidBloberAccount(format!("Failed to deserialize: {e:?}"))
        })?;

        Ok(Some(blober))
    }

    /// Retrieves the checkpoint containing the Groth16 proof for a given blober account.
    pub async fn get_checkpoint(
        &self,
        blober: BloberIdentifier,
    ) -> DataAnchorClientResult<Option<Checkpoint>> {
        let blober = blober.to_blober_address(self.program_id, self.payer.pubkey());
        let checkpoint_address = find_checkpoint_address(self.program_id, blober);
        let account = self
            .rpc_client
            .get_account_with_commitment(&checkpoint_address, self.rpc_client.commitment())
            .await?
            .value;

        let Some(account) = account else {
            return Ok(None);
        };

        if account.owner != self.program_id {
            return Err(LedgerDataBlobError::AccountNotOwnedByProgram.into());
        }

        if !account.data.starts_with(Checkpoint::DISCRIMINATOR) {
            return Err(LedgerDataBlobError::InvalidCheckpointAccount(
                "Invalid discriminator".to_owned(),
            )
            .into());
        }

        let mut state = account.data.get(Checkpoint::DISCRIMINATOR.len()..).ok_or(
            LedgerDataBlobError::InvalidCheckpointAccount("No state data".to_owned()),
        )?;

        if state.is_empty() {
            return Err(LedgerDataBlobError::InvalidCheckpointAccount(
                "Empty state data".to_owned(),
            )
            .into());
        }

        let checkpoint = Checkpoint::deserialize(&mut state).map_err(|e| {
            LedgerDataBlobError::InvalidCheckpointAccount(format!("Failed to deserialize: {e:?}"))
        })?;

        Ok(Some(checkpoint))
    }
}
