use data_anchor_api::{CompoundInclusionProof, IndexerRpcClient, ProofData, TimeRange};
use solana_pubkey::Pubkey;
use solana_sdk::{clock::Slot, signer::Signer};

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
}

impl DataAnchorClient {
    /// Fetches all blobs for a given slot from the [`IndexerRpcClient`].
    pub async fn get_blobs(
        &self,
        slot: u64,
        identifier: BloberIdentifier,
    ) -> DataAnchorClientResult<Option<Vec<Vec<u8>>>> {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        self.indexer()
            .get_blobs(blober.into(), slot)
            .await
            .map_err(|e| IndexerError::Blobs(slot, e.to_string()).into())
    }

    /// Fetches blobs for a given blober and time range from the [`IndexerRpcClient`].
    pub async fn get_blobs_by_blober(
        &self,
        identifier: BloberIdentifier,
        time_range: Option<TimeRange>,
    ) -> DataAnchorClientResult<Vec<Vec<u8>>> {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        self.indexer()
            .get_blobs_by_blober(blober.into(), time_range)
            .await
            .map_err(|e| IndexerError::BlobsForBlober(blober.to_string(), e.to_string()).into())
    }

    /// Fetches blobs for a given payer, network name and time range from the [`IndexerRpcClient`].
    pub async fn get_blobs_by_payer(
        &self,
        payer: Pubkey,
        network_name: String,
        time_range: Option<TimeRange>,
    ) -> DataAnchorClientResult<Vec<Vec<u8>>> {
        self.indexer()
            .get_blobs_by_payer(payer.into(), network_name, time_range)
            .await
            .map_err(|e| IndexerError::BlobsForPayer(payer.to_string(), e.to_string()).into())
    }

    /// Fetches blobs for a given network and time range from the [`IndexerRpcClient`].
    pub async fn get_blobs_by_network(
        &self,
        network_name: String,
        time_range: Option<TimeRange>,
    ) -> DataAnchorClientResult<Vec<Vec<u8>>> {
        self.indexer()
            .get_blobs_by_network(network_name.clone(), time_range)
            .await
            .map_err(|e| IndexerError::BlobsForNetwork(network_name, e.to_string()).into())
    }

    /// Fetches blobs for a given namespace and time range from the [`IndexerRpcClient`].
    pub async fn get_blobs_by_namespace_for_payer(
        &self,
        namespace: String,
        payer_pubkey: Option<Pubkey>,
        time_range: Option<TimeRange>,
    ) -> DataAnchorClientResult<Vec<Vec<u8>>> {
        self.indexer()
            .get_blobs_by_namespace_for_payer(
                namespace.clone(),
                payer_pubkey.map(|p| p.into()),
                time_range,
            )
            .await
            .map_err(|e| IndexerError::BlobsForNamespace(namespace, e.to_string()).into())
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

    /// Requests ZK proof generation on the indexer for a given blober and slot.
    pub async fn checkpoint_proof(
        &self,
        slot: Slot,
        identifier: BloberIdentifier,
    ) -> DataAnchorClientResult<ProofData> {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        self.indexer()
            .checkpoint_proof(blober.into(), slot)
            .await
            .map_err(|e| IndexerError::ZKProof(blober.to_string(), slot, e.to_string()).into())
    }
}
