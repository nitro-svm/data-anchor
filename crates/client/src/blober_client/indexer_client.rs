use std::time::Duration;

use data_anchor_api::{BlobsByBlober, BlobsByPayer, CompoundProof, IndexerRpcClient};
use data_anchor_blober::find_blober_address;
use solana_sdk::{pubkey::Pubkey, signer::Signer};

use crate::{BloberClient, BloberClientResult, IndexerError};

impl BloberClient {
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
                .get_blobs(blober.into(), slot)
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
                .get_proof(blober.into(), slot)
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
            .get_proof_for_blob(blob.into())
            .await
            .map_err(|e| IndexerError::ProofForBlob(blob.to_string(), e.to_string()).into())
    }
}
