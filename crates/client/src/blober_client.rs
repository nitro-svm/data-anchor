use std::{sync::Arc, time::Duration};

use anchor_lang::{Discriminator, Space};
use blober::{find_blob_address, state::blober::Blober, CHUNK_SIZE};
use bon::Builder;
use jsonrpsee::ws_client::WsClient;
use nitro_da_indexer_api::{CompoundProof, IndexerRpcClient};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::config::RpcBlockConfig;
use solana_sdk::{
    hash::Hash, message::VersionedMessage, pubkey::Pubkey, signature::Keypair, signer::Signer,
};
use solana_transaction_status::{EncodedConfirmedBlock, UiTransactionEncoding};
use tracing::{info_span, Instrument, Span};

use crate::{
    batch_client::{BatchClient, SuccessfulTransaction},
    fees::{Fee, FeeStrategy, Lamports, Priority},
    helpers::{
        check_outcomes, find_finalize_blob_transactions_for_blober, get_unique_timestamp,
        split_blob_into_chunks,
    },
    tx::{self, set_compute_unit_price::calculate_compute_unit_price, MessageArguments},
    types::{IndexerError, TransactionType, UploadBlobError},
    BloberClientError, BloberClientResult,
};

#[derive(Builder, Clone)]
pub struct BloberClient {
    pub(crate) payer: Arc<Keypair>,
    pub(crate) program_id: Pubkey,
    pub(crate) rpc_client: Arc<RpcClient>,
    pub(crate) batch_client: BatchClient,
    // Optional for the sake of testing, because in some tests indexer client is not used
    pub(crate) indexer_client: Option<Arc<WsClient>>,
}

impl BloberClient {
    /// Creates a new `BloberClient` with the given payer and RPC client.
    pub fn new(
        payer: Arc<Keypair>,
        program_id: Pubkey,
        rpc_client: Arc<RpcClient>,
        batch_client: BatchClient,
        indexer_client: Option<Arc<WsClient>>,
    ) -> Self {
        Self {
            payer,
            program_id,
            rpc_client,
            batch_client,
            indexer_client,
        }
    }

    pub async fn upload_blob(
        &self,
        blob_data: &[u8],
        fee_strategy: FeeStrategy,
        blober: Pubkey,
        timeout: Option<Duration>,
    ) -> BloberClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
        let timestamp = get_unique_timestamp();

        let blob = find_blob_address(self.payer.pubkey(), timestamp);

        // Convert priority-based fee strategy to a fixed fee by calculating once up-front.
        let fee_strategy = self
            .convert_fee_strategy_to_fixed(fee_strategy, blob)
            .in_current_span()
            .await?;

        let chunks = split_blob_into_chunks(blob_data);

        let (declare_blob_msg, insert_chunks_msgs, finalize_blob_msg) = self
            .generate_messages(blob, timestamp, chunks, fee_strategy, blober)
            .await;

        let res = self
            .do_upload(
                declare_blob_msg,
                insert_chunks_msgs,
                finalize_blob_msg,
                timeout,
            )
            .in_current_span()
            .await;

        if let Err(BloberClientError::UploadBlob(UploadBlobError::DeclareBlob(_))) = res {
            self.discard_blob(fee_strategy, blob, blober, timeout).await
        } else {
            res
        }
    }

    pub async fn discard_blob(
        &self,
        fee_strategy: FeeStrategy,
        blob: Pubkey,
        blober: Pubkey,
        timeout: Option<Duration>,
    ) -> BloberClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
        let fee_strategy = self
            .convert_fee_strategy_to_fixed(fee_strategy, blob)
            .in_current_span()
            .await?;

        let msg = tx::discard_blob(
            &MessageArguments::new(
                self.program_id,
                blober,
                &self.payer,
                self.rpc_client.clone(),
                fee_strategy,
            ),
            blob,
        )
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

    pub async fn estimate_fees(
        &self,
        blob_size: usize,
        priority: Priority,
    ) -> BloberClientResult<Fee> {
        // This whole functions is basically a simulation that doesn't run anything. Instead of executing transactions,
        // it just sums the expected fees and number of signatures.

        // The blob account is always newly created, so for estimating compute fees
        // we don't even need the real keypair, any unused pubkey will do.
        let fake_pubkey = Keypair::new().pubkey();
        let prioritization_fee_rate = calculate_compute_unit_price(
            &self.rpc_client,
            &[fake_pubkey, self.payer.pubkey()],
            priority,
        )
        .await?;
        let mut compute_unit_limit = 0;
        let mut num_signatures = 0;

        compute_unit_limit += tx::declare_blob::COMPUTE_UNIT_LIMIT;
        num_signatures += tx::declare_blob::NUM_SIGNATURES;

        let num_chunks = blob_size.div_ceil(CHUNK_SIZE as usize) as u16;
        compute_unit_limit += num_chunks as u32 * tx::insert_chunk::COMPUTE_UNIT_LIMIT;
        num_signatures += num_chunks * tx::insert_chunk::NUM_SIGNATURES;

        compute_unit_limit += tx::finalize_blob::COMPUTE_UNIT_LIMIT;
        num_signatures += tx::finalize_blob::NUM_SIGNATURES;

        Ok(Fee {
            num_signatures,
            // The base Solana transaction fee = 5000.
            // Reference link: https://solana.com/docs/core/fees#:~:text=While%20transaction%20fees%20are%20paid,of%205k%20lamports%20per%20signature.
            price_per_signature: Lamports::new(5000),
            compute_unit_limit,
            prioritization_fee_rate,
            blob_account_size: Blober::DISCRIMINATOR.len() + Blober::INIT_SPACE,
        })
    }

    /// Fetches all blobs for a given slot.
    pub async fn get_blobs(&self, slot: u64, blober: Pubkey) -> BloberClientResult<Vec<Vec<u8>>> {
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

    /// Fetches compound proof for a given slot.
    pub async fn get_slot_proof(
        &self,
        slot: u64,
        blober: Pubkey,
    ) -> BloberClientResult<CompoundProof> {
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

    /// Fetches blob hashes for a given slot
    pub async fn get_blob_hashes(
        &self,
        slot: u64,
        blober: Pubkey,
    ) -> BloberClientResult<Vec<Hash>> {
        let messages = self.get_blob_messages(slot, blober).await?;
        let hashes = messages
            .into_iter()
            .map(|(_blob, message)| message.hash())
            .collect();
        Ok(hashes)
    }

    /// Fetches blob messages for a given slot
    /// Returns a tuple of (Pubkey, VersionedMessage)
    pub async fn get_blob_messages(
        &self,
        slot: u64,
        blober: Pubkey,
    ) -> BloberClientResult<Vec<(Pubkey, VersionedMessage)>> {
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
        // Directly pass the closure returned by the function
        let messages = block
            .transactions
            .iter()
            .filter_map(find_finalize_blob_transactions_for_blober(blober))
            .collect::<Vec<_>>();
        Ok(messages)
    }
}
