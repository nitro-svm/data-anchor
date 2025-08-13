use anchor_lang::{prelude::Pubkey, solana_program::clock::Slot};
use data_anchor_api::{CompoundInclusionProof, IndexerRpcClient, PubkeyFromStr, TimeRange};
use data_anchor_utils::{
    compression::DataAnchorCompressionAsync,
    encoding::{DataAnchorEncoding, Decodable},
};
use solana_signer::Signer;

use super::BloberIdentifier;
use crate::{DataAnchorClient, DataAnchorClientResult};

#[derive(thiserror::Error, Debug)]
pub enum IndexerError {
    /// Failed to read blobs for slot {0} via indexer client: {1}
    #[error("Failed to read blobs for slot {0} via indexer client: {1}")]
    Blobs(Slot, String),
    /// Failed to read proof for slot {0} via indexer client: {1}
    #[error("Failed to read proof for slot {0} via indexer client: {1}")]
    Proof(Slot, String),
    /// Failed to read blobs for blober {0} via indexer client: {1}
    #[error("Failed to read blobs for blober {0} via indexer client: {1}")]
    BlobsForBlober(String, String),
    /// Failed to read blobs for payer {0} via indexer client: {1}
    #[error("Failed to read blobs for payer {0} via indexer client: {1}")]
    BlobsForPayer(String, String),
    /// Failed to read blobs for network {0} via indexer client: {1}
    #[error("Failed to read blobs for network {0} via indexer client: {1}")]
    BlobsForNetwork(String, String),
    /// Failed to read blobs for namespace {0} via indexer client: {1}
    #[error("Failed to read blobs for namespace {0} via indexer client: {1}")]
    BlobsForNamespace(String, String),
    /// Failed to read proof for blob {0} via indexer client: {1}
    #[error("Failed to read proof for blob {0} via indexer client: {1}")]
    ProofForBlob(String, String),
    /// Failed to read compound proof for slot {0} via indexer client: {1}
    #[error("Failed to read checkpoint proof for blober {0} and slot {1} via indexer client: {2}")]
    ZKProof(String, u64, String),
    /// Failed to read payers for network {0} via indexer client: {1}
    #[error("Failed to read payers for network {0} via indexer client: {1}")]
    PayersForNamespace(String, String),
}

impl<Encoding, Compression> DataAnchorClient<Encoding, Compression>
where
    Encoding: DataAnchorEncoding + Default,
    Compression: DataAnchorCompressionAsync,
{
    /// Fetches all blobs for a given slot from the [`IndexerRpcClient`].
    pub async fn get_blobs<T>(
        &self,
        slot: u64,
        identifier: BloberIdentifier,
    ) -> DataAnchorClientResult<Option<Vec<T>>>
    where
        T: Decodable,
    {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        let Some(blobs) = self
            .indexer()
            .get_blobs(blober.into(), slot)
            .await
            .map_err(|e| IndexerError::Blobs(slot, e.to_string()))?
        else {
            return Ok(None);
        };

        self.decompress_and_decode_vec(blobs.iter().map(|b| b.as_slice()))
            .await
            .map(Some)
    }

    /// Fetches blobs for a given blober and time range from the [`IndexerRpcClient`].
    pub async fn get_blobs_by_blober<T>(
        &self,
        identifier: BloberIdentifier,
        time_range: Option<TimeRange>,
    ) -> DataAnchorClientResult<Vec<T>>
    where
        T: Decodable,
    {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        let blobs = self
            .indexer()
            .get_blobs_by_blober(blober.into(), time_range)
            .await
            .map_err(|e| IndexerError::BlobsForBlober(blober.to_string(), e.to_string()))?;

        self.decompress_and_decode_vec(blobs.iter().map(|b| b.as_slice()))
            .await
    }

    /// Fetches blobs for a given payer, network name and time range from the [`IndexerRpcClient`].
    pub async fn get_blobs_by_payer<T>(
        &self,
        payer: Pubkey,
        network_name: String,
        time_range: Option<TimeRange>,
    ) -> DataAnchorClientResult<Vec<T>>
    where
        T: Decodable,
    {
        let blobs = self
            .indexer()
            .get_blobs_by_payer(payer.into(), network_name, time_range)
            .await
            .map_err(|e| IndexerError::BlobsForPayer(payer.to_string(), e.to_string()))?;

        self.decompress_and_decode_vec(blobs.iter().map(|b| b.as_slice()))
            .await
    }

    /// Fetches blobs for a given network and time range from the [`IndexerRpcClient`].
    pub async fn get_blobs_by_network<T>(
        &self,
        network_name: String,
        time_range: Option<TimeRange>,
    ) -> DataAnchorClientResult<Vec<T>>
    where
        T: Decodable,
    {
        let blobs = self
            .indexer()
            .get_blobs_by_network(network_name.clone(), time_range)
            .await
            .map_err(|e| IndexerError::BlobsForNetwork(network_name, e.to_string()))?;

        self.decompress_and_decode_vec(blobs.iter().map(|b| b.as_slice()))
            .await
    }

    /// Fetches blobs for a given namespace and time range from the [`IndexerRpcClient`].
    pub async fn get_blobs_by_namespace_for_payer<T>(
        &self,
        namespace: String,
        payer_pubkey: Option<Pubkey>,
        time_range: Option<TimeRange>,
    ) -> DataAnchorClientResult<Vec<T>>
    where
        T: Decodable,
    {
        let blobs = self
            .indexer()
            .get_blobs_by_namespace_for_payer(
                namespace.clone(),
                payer_pubkey.map(|p| p.into()),
                time_range,
            )
            .await
            .map_err(|e| IndexerError::BlobsForNamespace(namespace, e.to_string()))?;

        self.decompress_and_decode_vec(blobs.iter().map(|b| b.as_slice()))
            .await
    }

    /// Fetches payers for a given network from the [`IndexerRpcClient`].
    pub async fn get_payers_by_network(
        &self,
        network: String,
    ) -> DataAnchorClientResult<Vec<PubkeyFromStr>> {
        self.indexer()
            .get_payers_by_network(network.clone())
            .await
            .map_err(|e| IndexerError::PayersForNamespace(network, e.to_string()).into())
    }

    /// Fetches compound proof for a given slot from the [`IndexerRpcClient`].
    pub async fn get_proof(
        &self,
        slot: u64,
        identifier: BloberIdentifier,
    ) -> DataAnchorClientResult<Option<CompoundInclusionProof>> {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        self.indexer()
            .get_proof(blober.into(), slot)
            .await
            .map_err(|e| IndexerError::Proof(slot, e.to_string()).into())
    }

    /// Fetches compound proof for a given blob PDA [`Pubkey`] from the [`IndexerRpcClient`].
    pub async fn get_proof_for_blob(
        &self,
        blob: Pubkey,
    ) -> DataAnchorClientResult<Option<CompoundInclusionProof>> {
        self.indexer()
            .get_proof_for_blob(blob.into())
            .await
            .map_err(|e| IndexerError::ProofForBlob(blob.to_string(), e.to_string()).into())
    }
}
