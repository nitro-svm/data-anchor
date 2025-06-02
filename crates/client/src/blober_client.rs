use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use anchor_lang::{Discriminator, Space};
use blober_client_builder::{IsSet, IsUnset, SetHeliusFeeEstimate, SetIndexerClient};
use bon::Builder;
use data_anchor_api::{
    extract_relevant_instructions, get_account_at_index, BlobsByBlober, BlobsByPayer,
    CompoundProof, IndexerRpcClient, RelevantInstruction, RelevantInstructionWithAccounts,
};
use data_anchor_blober::{
    find_blob_address, find_blober_address,
    instruction::{Close, DeclareBlob, DiscardBlob, FinalizeBlob, Initialize, InsertChunk},
    state::blober::Blober,
    CHUNK_SIZE, COMPOUND_DECLARE_TX_SIZE, COMPOUND_TX_SIZE,
};
use futures::StreamExt;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use solana_cli_config::Config;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::config::RpcBlockConfig;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    message::VersionedMessage,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
};
use solana_transaction_status::{EncodedConfirmedBlock, UiTransactionEncoding};
use tracing::{info_span, Instrument, Span};

use crate::{
    batch_client::{BatchClient, SuccessfulTransaction},
    constants::{DEFAULT_CONCURRENCY, DEFAULT_LOOKBACK_SLOTS},
    fees::{Fee, FeeStrategy, Lamports, Priority},
    helpers::{
        check_outcomes, filter_relevant_instructions, get_blob_data_from_instructions,
        get_unique_timestamp,
    },
    tx::{Compound, CompoundDeclare, CompoundFinalize, MessageArguments, MessageBuilder},
    types::{IndexerError, TransactionType, UploadBlobError},
    BloberClientError, BloberClientResult, LedgerDataBlobError,
};

#[derive(Builder, Clone)]
pub struct BloberClient {
    #[builder(getter(name = get_payer, vis = ""))]
    pub(crate) payer: Arc<Keypair>,
    pub(crate) program_id: Pubkey,
    pub(crate) rpc_client: Arc<RpcClient>,
    pub(crate) batch_client: BatchClient,
    // Optional for the sake of testing, because in some tests indexer client is not used
    pub(crate) indexer_client: Option<Arc<WsClient>>,
    #[builder(default = false)]
    pub(crate) helius_fee_estimate: bool,
}

impl<State: blober_client_builder::State> BloberClientBuilder<State> {
    /// Adds an indexer client to the builder based on the given indexer URL.
    ///
    /// # Example
    ///
    /// ```rust
    /// use blober_client::BloberClient;
    ///
    /// let builder_with_indexer = BloberClient::builder()
    ///     .indexer_from_url("ws://localhost:8080")
    ///     .await?;
    /// ```
    pub async fn indexer_from_url(
        self,
        indexer_url: &str,
    ) -> BloberClientResult<BloberClientBuilder<SetIndexerClient<State>>>
    where
        State::IndexerClient: IsUnset,
    {
        let indexer_client = WsClientBuilder::new().build(indexer_url).await?;
        Ok(self.indexer_client(Arc::new(indexer_client)))
    }

    /// Builds a new `BloberClient` with an RPC client and a batch client built from the given
    /// Solana cli [`Config`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::sync::Arc;
    ///
    /// use blober_client::{BloberClient};
    /// use solana_cli_config::Config;
    /// use solana_sdk::{pubkey::Pubkey, signature::Keypair};
    ///
    /// let payer = Arc::new(Keypair::new());
    /// let program_id = Pubkey::new_unique();
    /// let solana_config = Config::default();
    /// let client = BloberClient::builder()
    ///     .payer(payer)
    ///     .program_id(program_id)
    ///     .build_with_config(solana_config)
    ///     .await?;
    /// ```
    pub async fn build_with_config(self, solana_config: Config) -> BloberClientResult<BloberClient>
    where
        State::Payer: IsSet,
        State::ProgramId: IsSet,
        State::RpcClient: IsUnset,
        State::BatchClient: IsUnset,
    {
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            solana_config.json_rpc_url.clone(),
            CommitmentConfig::from_str(&solana_config.commitment)?,
        ));
        let payer = self.get_payer().clone();
        Ok(self
            .rpc_client(rpc_client.clone())
            .batch_client(BatchClient::new(rpc_client.clone(), vec![payer.clone()]).await?)
            .build())
    }

    pub fn with_helius_fee_estimate(self) -> BloberClientBuilder<SetHeliusFeeEstimate<State>>
    where
        State::HeliusFeeEstimate: IsUnset,
    {
        self.helius_fee_estimate(true)
    }
}

impl BloberClient {
    /// Returns the underlaying [`RpcClient`].
    pub fn rpc_client(&self) -> Arc<RpcClient> {
        self.rpc_client.clone()
    }

    /// Returns the transaction payer [`Keypair`].
    pub fn payer(&self) -> Arc<Keypair> {
        self.payer.clone()
    }

    /// Initializes a new [`Blober`] PDA account.
    pub async fn initialize_blober(
        &self,
        fee_strategy: FeeStrategy,
        namespace: &str,
        timeout: Option<Duration>,
    ) -> BloberClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
        let blober = find_blober_address(self.program_id, self.payer.pubkey(), namespace);

        let fee_strategy = self
            .convert_fee_strategy_to_fixed(
                fee_strategy,
                &[blober],
                TransactionType::InitializeBlober,
            )
            .in_current_span()
            .await?;

        let msg = Initialize::build_message(MessageArguments::new(
            self.program_id,
            blober,
            &self.payer,
            self.rpc_client.clone(),
            fee_strategy,
            self.helius_fee_estimate,
            (namespace.to_owned(), blober),
        ))
        .await
        .expect("infallible with a fixed fee strategy");

        let span = info_span!(parent: Span::current(), "initialize_blober");
        Ok(check_outcomes(
            self.batch_client
                .send(vec![(TransactionType::InitializeBlober, msg)], timeout)
                .instrument(span)
                .await,
        )
        .map_err(UploadBlobError::InitializeBlober)?)
    }

    /// Closes a [`Blober`] PDA account.
    pub async fn close_blober(
        &self,
        fee_strategy: FeeStrategy,
        namespace: &str,
        timeout: Option<Duration>,
    ) -> BloberClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
        let blober = find_blober_address(self.program_id, self.payer.pubkey(), namespace);

        let fee_strategy = self
            .convert_fee_strategy_to_fixed(fee_strategy, &[blober], TransactionType::CloseBlober)
            .in_current_span()
            .await?;

        let msg = Close::build_message(MessageArguments::new(
            self.program_id,
            blober,
            &self.payer,
            self.rpc_client.clone(),
            fee_strategy,
            self.helius_fee_estimate,
            (),
        ))
        .await
        .expect("infallible with a fixed fee strategy");

        let span = info_span!(parent: Span::current(), "close_blober");
        Ok(check_outcomes(
            self.batch_client
                .send(vec![(TransactionType::CloseBlober, msg)], timeout)
                .instrument(span)
                .await,
        )
        .map_err(UploadBlobError::CloseBlober)?)
    }

    /// Uploads a blob of data with the given [`Blober`] PDA account.
    /// Under the hood it creates a new [`blober::state::blob::Blob`] PDA which stores a incremental hash of the chunks
    /// from the blob data. On completion of the blob upload, the blob PDA gets closed sending it's
    /// funds back to the [`BloberClient::payer`].
    /// If the blob upload fails, the blob PDA gets discarded and the funds also get sent to the
    /// [`BloberClient::payer`].
    pub async fn upload_blob(
        &self,
        blob_data: &[u8],
        fee_strategy: FeeStrategy,
        namespace: &str,
        timeout: Option<Duration>,
    ) -> BloberClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
        let blober = find_blober_address(self.program_id, self.payer.pubkey(), namespace);
        let timestamp = get_unique_timestamp();

        let blob = find_blob_address(
            self.program_id,
            self.payer.pubkey(),
            blober,
            timestamp,
            blob_data.len(),
        );

        let upload_messages = self
            .generate_messages(blob, timestamp, blob_data, fee_strategy, blober)
            .await?;

        let res = self
            .do_upload(upload_messages, timeout)
            .in_current_span()
            .await;

        if let Err(BloberClientError::UploadBlob(UploadBlobError::DeclareBlob(_))) = res {
            self.discard_blob(fee_strategy, blob, namespace, timeout)
                .await
        } else {
            res
        }
    }

    /// Discards a [`blober::state::blob::Blob`] PDA account registered with the provided
    /// [`Blober`] PDA account.
    pub async fn discard_blob(
        &self,
        fee_strategy: FeeStrategy,
        blob: Pubkey,
        namespace: &str,
        timeout: Option<Duration>,
    ) -> BloberClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
        let blober = find_blober_address(self.program_id, self.payer.pubkey(), namespace);

        let fee_strategy = self
            .convert_fee_strategy_to_fixed(fee_strategy, &[blob], TransactionType::DiscardBlob)
            .in_current_span()
            .await?;

        let msg = DiscardBlob::build_message(MessageArguments::new(
            self.program_id,
            blober,
            &self.payer,
            self.rpc_client.clone(),
            fee_strategy,
            self.helius_fee_estimate,
            blob,
        ))
        .in_current_span()
        .await
        .expect("infallible with a fixed fee strategy");

        let span = info_span!(parent: Span::current(), "discard_blob");

        Ok(check_outcomes(
            self.batch_client
                .send(vec![(TransactionType::DiscardBlob, msg)], timeout)
                .instrument(span)
                .await,
        )
        .map_err(UploadBlobError::DiscardBlob)?)
    }

    /// Estimates fees for uploading a blob of the size `blob_size` with the given `priority`.
    /// This whole functions is basically a simulation that doesn't run anything. Instead of executing transactions,
    /// it just sums the expected fees and number of signatures.
    ///
    /// The [`blober::state::blob::Blob`] PDA account is always newly created, so for estimating compute fees
    /// we don't even need the real keypair, any unused pubkey will do.
    pub async fn estimate_fees(
        &self,
        blob_size: usize,
        blober: Pubkey,
        priority: Priority,
    ) -> BloberClientResult<Fee> {
        let prioritization_fee_rate = priority
            .get_priority_fee_estimate(
                &self.rpc_client,
                &[Pubkey::new_unique(), blober, self.payer.pubkey()],
                self.helius_fee_estimate,
            )
            .await?;

        let num_chunks = blob_size.div_ceil(CHUNK_SIZE as usize) as u16;

        let (compute_unit_limit, num_signatures) = if blob_size < COMPOUND_TX_SIZE as usize {
            (Compound::COMPUTE_UNIT_LIMIT, Compound::NUM_SIGNATURES)
        } else if blob_size < COMPOUND_DECLARE_TX_SIZE as usize {
            (
                CompoundDeclare::COMPUTE_UNIT_LIMIT + FinalizeBlob::COMPUTE_UNIT_LIMIT,
                CompoundDeclare::NUM_SIGNATURES + FinalizeBlob::NUM_SIGNATURES,
            )
        } else {
            (
                DeclareBlob::COMPUTE_UNIT_LIMIT
                    + (num_chunks - 1) as u32 * InsertChunk::COMPUTE_UNIT_LIMIT
                    + CompoundFinalize::COMPUTE_UNIT_LIMIT,
                DeclareBlob::NUM_SIGNATURES
                    + (num_chunks - 1) * InsertChunk::NUM_SIGNATURES
                    + CompoundFinalize::NUM_SIGNATURES,
            )
        };

        // The base Solana transaction fee = 5000.
        // Reference link: https://solana.com/docs/core/fees#:~:text=While%20transaction%20fees%20are%20paid,of%205k%20lamports%20per%20signature.
        let price_per_signature = Lamports::new(5000);

        let blob_account_size = Blober::DISCRIMINATOR.len() + Blober::INIT_SPACE;

        Ok(Fee {
            num_signatures,
            price_per_signature,
            compute_unit_limit,
            prioritization_fee_rate,
            blob_account_size,
        })
    }

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

    /// Fetches all blobs for a given slot from the [`IndexerRpcClient`].
    pub async fn get_blobs(
        &self,
        slot: u64,
        namespace: &str,
        payer_pubkey: Option<Pubkey>,
    ) -> BloberClientResult<Vec<Vec<u8>>> {
        let payer_pubkey = payer_pubkey.unwrap_or(self.payer.pubkey());
        let blober = find_blober_address(self.program_id, payer_pubkey, namespace);

        loop {
            let blobs = self
                .indexer()
                .get_blobs(blober, slot)
                .await
                .map_err(|e| IndexerError::Blobs(slot, e.to_string()))?;
            if let Some(blobs) = blobs {
                return Ok(blobs);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Fetches blobs for a given [`BlobsByBlober`] from the [`IndexerRpcClient`].
    pub async fn get_blobs_by_blober(
        &self,
        blober_blobs: BlobsByBlober,
    ) -> BloberClientResult<Vec<Vec<u8>>> {
        let blober = blober_blobs.blober;

        self.indexer()
            .get_blobs_by_blober(blober_blobs)
            .await
            .map_err(|e| IndexerError::BlobsForBlober(blober.to_string(), e.to_string()).into())
    }

    /// Fetches blobs for a given [`BlobsByPayer`] from the [`IndexerRpcClient`].
    pub async fn get_blobs_by_payer(
        &self,
        payer_blobs: BlobsByPayer,
    ) -> BloberClientResult<Vec<Vec<u8>>> {
        let payer = payer_blobs.payer;

        self.indexer()
            .get_blobs_by_payer(payer_blobs)
            .await
            .map_err(|e| IndexerError::BlobsForPayer(payer.to_string(), e.to_string()).into())
    }

    /// Fetches compound proof for a given slot from the [`IndexerRpcClient`].
    pub async fn get_slot_proof(
        &self,
        slot: u64,
        namespace: &str,
        payer_pubkey: Option<Pubkey>,
    ) -> BloberClientResult<CompoundProof> {
        let payer_pubkey = payer_pubkey.unwrap_or(self.payer.pubkey());
        let blober = find_blober_address(self.program_id, payer_pubkey, namespace);

        loop {
            let proof = self
                .indexer()
                .get_proof(blober, slot)
                .await
                .map_err(|e| IndexerError::Proof(slot, e.to_string()))?;
            if let Some(proofs) = proof {
                return Ok(proofs);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Fetches compound proof for a given blob PDA [`Pubkey`] from the [`IndexerRpcClient`].
    pub async fn get_blob_proof(&self, blob: Pubkey) -> BloberClientResult<Option<CompoundProof>> {
        self.indexer()
            .get_proof_for_blob(blob)
            .await
            .map_err(|e| IndexerError::ProofForBlob(blob.to_string(), e.to_string()).into())
    }

    /// Fetches blob messages for a given slot
    /// Returns a tuple of ([`Pubkey`], [`VersionedMessage`]) where the Pubkey is the address of
    /// the [`blober::state::blob::Blob`] account and the VersionedMessage is the message that
    /// included the [`blober::instruction::FinalizeBlob`] instruction.
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
                            blob: get_account_at_index(&tx, compiled_instruction, 0)?,
                            blober: get_account_at_index(&tx, compiled_instruction, 1)?,
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
