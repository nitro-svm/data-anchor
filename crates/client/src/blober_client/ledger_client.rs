use std::collections::{HashMap, HashSet};

use data_anchor_api::{
    extract_relevant_instructions, get_account_at_index, RelevantInstruction,
    RelevantInstructionWithAccounts,
};
use data_anchor_blober::{
    find_blober_address, BLOB_ACCOUNT_INSTRUCTION_IDX, BLOB_BLOBER_INSTRUCTION_IDX,
};
use futures::StreamExt;
use solana_client::rpc_config::{RpcBlockConfig, RpcTransactionConfig};
use solana_sdk::{message::VersionedMessage, pubkey::Pubkey, signature::Signature, signer::Signer};
use solana_transaction_status::{EncodedConfirmedBlock, UiTransactionEncoding};

use crate::{
    blober_client::get_blob_data_from_instructions,
    constants::{DEFAULT_CONCURRENCY, DEFAULT_LOOKBACK_SLOTS},
    helpers::filter_relevant_instructions,
    BloberClient, BloberClientResult, LedgerDataBlobError,
};

impl BloberClient {
    /// Returns the raw blob data from the ledger for the given signatures.
    pub async fn get_ledger_blobs_from_signatures(
        &self,
        namespace: &str,
        payer_pubkey: Option<Pubkey>,
        signatures: Vec<Signature>,
    ) -> BloberClientResult<Vec<u8>> {
        let payer_pubkey = payer_pubkey.unwrap_or(self.payer.pubkey());
        let blober = find_blober_address(self.program_id, payer_pubkey, namespace);

        let relevant_transactions = futures::stream::iter(signatures)
            .map(|signature| async move {
                self.rpc_client
                    .get_transaction_with_config(
                        &signature,
                        RpcTransactionConfig {
                            commitment: Some(self.rpc_client.commitment()),
                            encoding: Some(UiTransactionEncoding::Base58),
                            ..Default::default()
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

        Ok(get_blob_data_from_instructions(
            &relevant_instructions,
            blober,
            *blob,
        )?)
    }

    /// Fetches all blobs finalized in a given slot from the ledger.
    pub async fn get_ledger_blobs(
        &self,
        slot: u64,
        namespace: &str,
        payer_pubkey: Option<Pubkey>,
        lookback_slots: Option<u64>,
    ) -> BloberClientResult<Vec<Vec<u8>>> {
        let payer_pubkey = payer_pubkey.unwrap_or(self.payer.pubkey());
        let blober = find_blober_address(self.program_id, payer_pubkey, namespace);

        let block_config = RpcBlockConfig {
            commitment: Some(self.rpc_client.commitment()),
            encoding: Some(UiTransactionEncoding::Base58),
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
            return Ok(blobs.values().cloned().collect());
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
                println!("total {}", instructions.len());

                if let Ok(blob_data) = get_blob_data_from_instructions(instructions, blober, *blob)
                {
                    blobs.insert(blob, blob_data);
                }
            }
            if blobs.len() == finalized_blobs.len() {
                break;
            }
        }

        Ok(blobs.values().cloned().collect())
    }

    /// Fetches blob messages for a given slot
    /// Returns a tuple of ([`Pubkey`], [`VersionedMessage`]) where the Pubkey is the address of
    /// the [`data_anchor_blober::state::blob::Blob`] account and the VersionedMessage is the message
    /// that included the [`data_anchor_blober::instruction::FinalizeBlob`] instruction.
    pub async fn get_blob_messages(
        &self,
        slot: u64,
        namespace: &str,
        payer_pubkey: Option<Pubkey>,
    ) -> BloberClientResult<Vec<(Pubkey, VersionedMessage)>> {
        let payer_pubkey = payer_pubkey.unwrap_or(self.payer.pubkey());
        let blober = find_blober_address(self.program_id, payer_pubkey, namespace);

        let block: EncodedConfirmedBlock = self
            .rpc_client
            .get_block_with_config(
                slot,
                RpcBlockConfig {
                    commitment: Some(self.rpc_client.commitment()),
                    encoding: Some(UiTransactionEncoding::Base58),
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
}
