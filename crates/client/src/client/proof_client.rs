use anchor_lang::solana_program::clock::Slot;
use data_anchor_api::{CustomerElf, ProofRpcClient, RequestStatus};
use data_anchor_utils::{compression::DataAnchorCompression, encoding::DataAnchorEncoding};
use solana_signer::Signer;

use super::BloberIdentifier;
use crate::{DataAnchorClient, DataAnchorClientResult};

#[derive(thiserror::Error, Debug)]
pub enum ProofError {
    /// Failed to read checkpoint proof for blober {0} and slot {1} with {2} via indexer client: {3}
    #[error(
        "Failed to read checkpoint proof for blober {0} and slot {1} with {2} via indexer client: {3}"
    )]
    ZKProof(String, u64, CustomerElf, String),
    /// Failed to get proof request status: {0}
    #[error("Failed to get proof request status for request ID {0}: {1}")]
    ProofRequestStatus(String, String),
}

impl<Encoding, Compression> DataAnchorClient<Encoding, Compression>
where
    Encoding: DataAnchorEncoding,
    Compression: DataAnchorCompression,
{
    /// Requests ZK proof generation on the proof RPC for a given blober, slot and proof type.
    pub async fn checkpoint_custom_proof(
        &self,
        slot: Slot,
        identifier: BloberIdentifier,
        customer_elf: CustomerElf,
    ) -> DataAnchorClientResult<String> {
        let blober = identifier.to_blober_address(self.program_id, self.payer.pubkey());

        self.proof()
            .checkpoint_proof(blober.into(), slot, customer_elf)
            .await
            .map_err(|e| {
                ProofError::ZKProof(blober.to_string(), slot, customer_elf, e.to_string()).into()
            })
    }

    /// Returns the status of a proof request by its request ID.
    pub async fn get_proof_request_status(
        &self,
        request_id: String,
    ) -> DataAnchorClientResult<RequestStatus> {
        self.proof()
            .get_proof_request_status(request_id.clone())
            .await
            .map_err(|e| ProofError::ProofRequestStatus(request_id, e.to_string()).into())
    }
}
