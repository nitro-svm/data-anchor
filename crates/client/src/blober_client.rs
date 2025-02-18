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
    transaction::Transaction,
};
use solana_transaction_status::{EncodedConfirmedBlock, UiTransactionEncoding};
use tracing::Instrument;

use crate::{
    batch_client::{BatchClient, SuccessfulTransaction},
    fees::{Fee, FeeStrategy, Lamports, Priority},
    helpers::{
        find_finalize_blob_transactions_for_blober, get_unique_timestamp, split_blob_into_chunks,
    },
    tx,
    tx::set_compute_unit_price::calculate_compute_unit_price,
    types::{IndexerError, TransactionType, UploadBlobError},
    Error,
};

#[derive(Builder, Clone)]
pub struct BloberClient {
    pub(crate) payer: Arc<Keypair>,
    pub(crate) rpc_client: Arc<RpcClient>,
    pub(crate) batch_client: BatchClient,
    // Optional for the sake of testing, because in some tests indexer client is not used
    pub(crate) indexer_client: Option<Arc<WsClient>>,
}

impl BloberClient {
    /// Creates a new `BloberClient` with the given payer and RPC client.
    pub fn new(
        payer: Arc<Keypair>,
        rpc_client: Arc<RpcClient>,
        batch_client: BatchClient,
        indexer_client: Option<Arc<WsClient>>,
    ) -> Self {
        Self {
            payer,
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
    ) -> Result<Vec<SuccessfulTransaction<TransactionType>>, UploadBlobError> {
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

        if let Err((true, err)) = res {
            // Client errors are not cloneable, and they need to be for the map_err calls to work.
            let err = Arc::new(err);
            // Last attempt to close the blob account.
            let msg = tx::discard_blob(&self.rpc_client, &self.payer, blob, blober, fee_strategy)
                .in_current_span()
                .await
                .expect("infallible with a fixed fee strategy");
            let blockhash = self
                .rpc_client
                .get_latest_blockhash()
                .in_current_span()
                .await
                .map_err(|err2| UploadBlobError::CloseAccount(err.clone(), err2))?;
            let tx = Transaction::new(&[&self.payer], msg, blockhash);
            self.rpc_client
                .send_and_confirm_transaction(&tx)
                .in_current_span()
                .await
                .map_err(|err2| UploadBlobError::CloseAccount(err.clone(), err2))?;
            Err(Arc::into_inner(err).expect("only one handle to the error"))
        } else {
            res.map_err(|(_, err)| err)
        }
    }

    pub async fn estimate_fees(&self, blob_size: usize, priority: Priority) -> Result<Fee, Error> {
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
        let mut compute_unit_limit = 0u32;
        let mut num_signatures = 0u16;

        compute_unit_limit += tx::declare_blob::COMPUTE_UNIT_LIMIT;
        num_signatures += tx::declare_blob::NUM_SIGNATURES as u16;

        let num_chunks = blob_size.div_ceil(CHUNK_SIZE as usize) as u16;
        compute_unit_limit += num_chunks as u32 * tx::insert_chunk::COMPUTE_UNIT_LIMIT;
        num_signatures += num_chunks * tx::insert_chunk::NUM_SIGNATURES as u16;

        compute_unit_limit += tx::finalize_blob::COMPUTE_UNIT_LIMIT;
        num_signatures += tx::finalize_blob::NUM_SIGNATURES as u16;

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
    pub async fn get_blobs(&self, slot: u64, blober: Pubkey) -> Result<Vec<Vec<u8>>, IndexerError> {
        let blobs = async move {
            loop {
                let blobs = self.indexer().get_blobs(blober, slot).await?;
                if let Some(blobs) = blobs {
                    return Ok(blobs);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        .await
        .map_err(|e: jsonrpsee::core::ClientError| {
            IndexerError::Blobs(format!(
                "Error when retrieving blobs for slot {}: {e:?}",
                slot
            ))
        })?;

        Ok(blobs)
    }

    /// Fetches compound proof for a given slot.
    pub async fn get_slot_proof(
        &self,
        slot: u64,
        blober: Pubkey,
    ) -> Result<CompoundProof, IndexerError> {
        let proof = async move {
            loop {
                let proof = self.indexer().get_proof(blober, slot).await?;
                if let Some(proofs) = proof {
                    return Ok(proofs);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        .await
        .map_err(|e: jsonrpsee::core::ClientError| {
            IndexerError::Proof(format!(
                "Error when retrieving proof for slot {}: {e:?}",
                slot
            ))
        })?;

        Ok(proof)
    }

    /// Fetches blob hashes for a given slot
    pub async fn get_blob_hashes(&self, slot: u64, blober: Pubkey) -> Result<Vec<Hash>, Error> {
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
    ) -> Result<Vec<(Pubkey, VersionedMessage)>, Error> {
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
