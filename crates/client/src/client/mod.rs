use std::{sync::Arc, time::Duration};

use anchor_lang::{Discriminator, Space};
use bon::Builder;
use data_anchor_blober::{
    CHUNK_SIZE, COMPOUND_DECLARE_TX_SIZE, COMPOUND_TX_SIZE, find_blob_address, find_blober_address,
    instruction::{Close, DeclareBlob, DiscardBlob, FinalizeBlob, Initialize, InsertChunk},
    state::blober::Blober,
};
use jsonrpsee::http_client::HttpClient;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use tracing::{Instrument, Span, info_span};

use crate::{
    DataAnchorClientError, DataAnchorClientResult,
    batch_client::{BatchClient, SuccessfulTransaction},
    fees::{Fee, FeeStrategy, Lamports, Priority},
    helpers::{check_outcomes, get_unique_timestamp},
    tx::{Compound, CompoundDeclare, CompoundFinalize, MessageArguments, MessageBuilder},
    types::{TransactionType, UploadBlobError},
};

mod builder;
mod indexer_client;
mod ledger_client;

#[derive(Builder, Clone)]
pub struct DataAnchorClient {
    #[builder(getter(name = get_payer, vis = ""))]
    pub(crate) payer: Arc<Keypair>,
    pub(crate) program_id: Pubkey,
    pub(crate) rpc_client: Arc<RpcClient>,
    pub(crate) batch_client: BatchClient,
    // Optional for the sake of testing, because in some tests indexer client is not used
    pub(crate) indexer_client: Option<Arc<HttpClient>>,
    #[builder(default = false)]
    pub(crate) helius_fee_estimate: bool,
}

impl DataAnchorClient {
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
    ) -> DataAnchorClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
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
    ) -> DataAnchorClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
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
    /// Under the hood it creates a new [`data_anchor_blober::state::blob::Blob`] PDA which stores a
    /// incremental hash of the chunks from the blob data. On completion of the blob upload, the
    /// blob PDA gets closed sending it's funds back to the [`DataAnchorClient::payer`].
    /// If the blob upload fails, the blob PDA gets discarded and the funds also get sent to the
    /// [`DataAnchorClient::payer`].
    pub async fn upload_blob(
        &self,
        blob_data: &[u8],
        fee_strategy: FeeStrategy,
        namespace: &str,
        timeout: Option<Duration>,
    ) -> DataAnchorClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
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

        if let Err(DataAnchorClientError::UploadBlob(UploadBlobError::DeclareBlob(_))) = res {
            self.discard_blob(fee_strategy, blob, namespace, timeout)
                .await
        } else {
            res
        }
    }

    /// Discards a [`data_anchor_blober::state::blob::Blob`] PDA account registered with the provided
    /// [`Blober`] PDA account.
    pub async fn discard_blob(
        &self,
        fee_strategy: FeeStrategy,
        blob: Pubkey,
        namespace: &str,
        timeout: Option<Duration>,
    ) -> DataAnchorClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
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
    /// The [`data_anchor_blober::state::blob::Blob`] PDA account is always newly created, so for estimating compute fees
    /// we don't even need the real keypair, any unused pubkey will do.
    pub async fn estimate_fees(
        &self,
        blob_size: usize,
        blober: Pubkey,
        priority: Priority,
    ) -> DataAnchorClientResult<Fee> {
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
}
