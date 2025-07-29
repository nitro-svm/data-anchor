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
use solana_sdk::{
    commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair, signer::Signer,
};
use tracing::{Instrument, Span, info_span};

use crate::{
    DataAnchorClientError, DataAnchorClientResult,
    batch_client::{BatchClient, SuccessfulTransaction},
    fees::{Fee, FeeStrategy, Lamports},
    helpers::{check_outcomes, get_unique_timestamp},
    tx::{Compound, CompoundDeclare, CompoundFinalize, MessageArguments, MessageBuilder},
    types::TransactionType,
};

mod builder;
mod indexer_client;
mod ledger_client;

pub use indexer_client::IndexerError;
pub use ledger_client::ChainError;

/// Identifier for a blober, which can be either a combination of payer and namespace or just a pubkey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BloberIdentifier {
    Namespace(String),
    PayerAndNamespace { payer: Pubkey, namespace: String },
    Pubkey(Pubkey),
}

#[derive(Debug, thiserror::Error)]
pub enum BloberIdentifierError {
    /// Error indicating that the blober identifier is missing.
    #[error(
        "Missing blober identifier: either namespace, namespace and payer or blober PDA must be provided."
    )]
    MissingBloberIdentifier,
}

impl TryFrom<(Option<String>, Option<Pubkey>)> for BloberIdentifier {
    type Error = BloberIdentifierError;

    fn try_from(
        (namespace, blober_pda): (Option<String>, Option<Pubkey>),
    ) -> Result<Self, Self::Error> {
        match (namespace, blober_pda) {
            (Some(namespace), None) => Ok(namespace.into()),
            (None, Some(pubkey)) => Ok(pubkey.into()),
            (Some(namespace), Some(payer)) => Ok((payer, namespace).into()),
            _ => Err(BloberIdentifierError::MissingBloberIdentifier),
        }
    }
}

impl From<String> for BloberIdentifier {
    fn from(namespace: String) -> Self {
        BloberIdentifier::Namespace(namespace)
    }
}

impl From<(Pubkey, String)> for BloberIdentifier {
    fn from((payer, namespace): (Pubkey, String)) -> Self {
        BloberIdentifier::PayerAndNamespace { payer, namespace }
    }
}

impl From<Pubkey> for BloberIdentifier {
    fn from(pubkey: Pubkey) -> Self {
        BloberIdentifier::Pubkey(pubkey)
    }
}

impl BloberIdentifier {
    /// Converts the [`BloberIdentifier`] to a [`Pubkey`] representing the blober address.
    pub fn to_blober_address(&self, program_id: Pubkey, payer: Pubkey) -> Pubkey {
        match self {
            BloberIdentifier::Namespace(namespace) => {
                find_blober_address(program_id, payer, namespace)
            }
            BloberIdentifier::PayerAndNamespace { payer, namespace } => {
                find_blober_address(program_id, *payer, namespace)
            }
            BloberIdentifier::Pubkey(pubkey) => *pubkey,
        }
    }

    /// Returns the namespace of the blober identifier.
    pub fn namespace(&self) -> Option<&str> {
        match self {
            BloberIdentifier::Namespace(namespace) => Some(namespace),
            BloberIdentifier::PayerAndNamespace { namespace, .. } => Some(namespace),
            BloberIdentifier::Pubkey(_) => None,
        }
    }
}

#[derive(Builder, Clone)]
pub struct DataAnchorClient {
    #[builder(getter(name = get_payer, vis = ""))]
    pub(crate) payer: Arc<Keypair>,
    #[builder(default = data_anchor_blober::id())]
    pub(crate) program_id: Pubkey,
    pub(crate) rpc_client: Arc<RpcClient>,
    pub(crate) batch_client: BatchClient,
    // Optional for the sake of testing, because in some tests indexer client is not used
    pub(crate) indexer_client: Option<Arc<HttpClient>>,
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

    fn in_mock_env(&self) -> bool {
        self.rpc_client.url().starts_with("MockSender")
    }

    async fn check_account_exists(&self, account: Pubkey) -> DataAnchorClientResult<bool> {
        Ok(self
            .rpc_client
            .get_account_with_commitment(&account, CommitmentConfig::confirmed())
            .await
            .map(|res| res.value.is_some())?)
    }

    async fn require_balance(&self, cost: Lamports) -> DataAnchorClientResult {
        let balance = self
            .rpc_client
            .get_balance_with_commitment(&self.payer.pubkey(), CommitmentConfig::confirmed())
            .await
            .map(|r| r.value)?;
        let cost_u64 = cost.into_inner() as u64;
        if balance < cost_u64 {
            return Err(ChainError::InsufficientBalance(balance, cost_u64).into());
        }
        Ok(())
    }

    /// Initializes a new [`Blober`] PDA account.
    pub async fn initialize_blober(
        &self,
        fee_strategy: FeeStrategy,
        identifier: BloberIdentifier,
        timeout: Option<Duration>,
    ) -> DataAnchorClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        let in_mock_env = self.in_mock_env();
        if !in_mock_env && self.check_account_exists(blober).await? {
            return Err(
                ChainError::AccountExists(format!("Blober PDA with address {blober}")).into(),
            );
        }

        let fee = fee_strategy
            .convert_fee_strategy_to_fixed(
                &self.rpc_client,
                &[blober, self.payer.pubkey()],
                TransactionType::InitializeBlober,
            )
            .in_current_span()
            .await?;

        if !in_mock_env {
            let cost = fee
                .total_fee()
                .checked_add(fee.rent())
                .ok_or_else(|| ChainError::CouldNotCalculateCost)?;
            self.require_balance(cost).await?;
        }

        let msg = Initialize::build_message(MessageArguments::new(
            self.program_id,
            blober,
            &self.payer,
            self.rpc_client.clone(),
            fee,
            (
                identifier
                    .namespace()
                    .ok_or(ChainError::MissingBloberNamespace)?
                    .to_owned(),
                blober,
            ),
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
        .map_err(ChainError::InitializeBlober)?)
    }

    /// Closes a [`Blober`] PDA account.
    pub async fn close_blober(
        &self,
        fee_strategy: FeeStrategy,
        identifier: BloberIdentifier,
        timeout: Option<Duration>,
    ) -> DataAnchorClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        let in_mock_env = self.in_mock_env();
        if !in_mock_env && !self.check_account_exists(blober).await? {
            return Err(ChainError::AccountDoesNotExist(format!(
                "Blober PDA with address {blober}"
            ))
            .into());
        }

        let fee = fee_strategy
            .convert_fee_strategy_to_fixed(
                &self.rpc_client,
                &[blober, self.payer.pubkey()],
                TransactionType::CloseBlober,
            )
            .in_current_span()
            .await?;

        if !in_mock_env {
            self.require_balance(fee.total_fee()).await?;
        }

        let msg = Close::build_message(MessageArguments::new(
            self.program_id,
            blober,
            &self.payer,
            self.rpc_client.clone(),
            fee,
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
        .map_err(ChainError::CloseBlober)?)
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
    ) -> DataAnchorClientResult<(Vec<SuccessfulTransaction<TransactionType>>, Pubkey)> {
        let blober = find_blober_address(self.program_id, self.payer.pubkey(), namespace);
        let timestamp = get_unique_timestamp();

        let blob = find_blob_address(
            self.program_id,
            self.payer.pubkey(),
            blober,
            timestamp,
            blob_data.len(),
        );

        let in_mock_env = self.in_mock_env();
        if !in_mock_env && self.check_account_exists(blob).await? {
            return Err(ChainError::AccountExists(format!("Blob PDA with address {blob}")).into());
        }

        let fee = self
            .estimate_fees(blob_data.len(), blober, fee_strategy)
            .await?;

        if !in_mock_env {
            let cost = fee
                .total_fee()
                .checked_add(fee.rent())
                .ok_or_else(|| ChainError::CouldNotCalculateCost)?;
            self.require_balance(cost).await?;
        }

        let upload_messages = self
            .generate_messages(blob, timestamp, blob_data, fee_strategy, blober)
            .await?;

        let res = self
            .do_upload(upload_messages, timeout)
            .in_current_span()
            .await;

        if let Err(DataAnchorClientError::ChainErrors(ChainError::DeclareBlob(_))) = res {
            self.discard_blob(fee_strategy, blob, namespace, timeout)
                .await
        } else {
            res.map(|r| (r, blob))
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
    ) -> DataAnchorClientResult<(Vec<SuccessfulTransaction<TransactionType>>, Pubkey)> {
        let blober = find_blober_address(self.program_id, self.payer.pubkey(), namespace);

        let in_mock_env = self.in_mock_env();
        if !in_mock_env && !self.check_account_exists(blob).await? {
            return Err(
                ChainError::AccountDoesNotExist(format!("Blob PDA with address {blob}")).into(),
            );
        }

        let fee = fee_strategy
            .convert_fee_strategy_to_fixed(
                &self.rpc_client,
                &[blob, self.payer.pubkey()],
                TransactionType::DiscardBlob,
            )
            .in_current_span()
            .await?;

        if !in_mock_env {
            self.require_balance(fee.total_fee()).await?;
        }

        let msg = DiscardBlob::build_message(MessageArguments::new(
            self.program_id,
            blober,
            &self.payer,
            self.rpc_client.clone(),
            fee,
            blob,
        ))
        .in_current_span()
        .await
        .expect("infallible with a fixed fee strategy");

        let span = info_span!(parent: Span::current(), "discard_blob");

        Ok((
            check_outcomes(
                self.batch_client
                    .send(vec![(TransactionType::DiscardBlob, msg)], timeout)
                    .instrument(span)
                    .await,
            )
            .map_err(ChainError::DiscardBlob)?,
            blob,
        ))
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
        fee_strategy: FeeStrategy,
    ) -> DataAnchorClientResult<Fee> {
        let prioritization_fee_rate = fee_strategy
            .convert_fee_strategy_to_fixed(
                &self.rpc_client,
                &[Pubkey::new_unique(), blober, self.payer.pubkey()],
                TransactionType::Compound,
            )
            .await?
            .prioritization_fee_rate;

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
