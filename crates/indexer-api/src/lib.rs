use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use nitro_da_proofs::compound::{
    completeness::CompoundCompletenessProof, inclusion::CompoundInclusionProof,
};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

/// A compound proof that proves whether a blob has been published in a specific slot.
/// See [`CompoundInclusionProof`] and [`CompoundCompletenessProof`] for more information.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum CompoundProof {
    /// See [`CompoundInclusionProof`].
    Inclusion(CompoundInclusionProof),
    /// See [`CompoundCompletenessProof`].
    Completeness(CompoundCompletenessProof),
}

/// The Indexer RPC interface.
#[rpc(server, client)]
pub trait IndexerRpc {
    /// Retrieve a list of blobs for a given slot and blober pubkey. Returns an error if there was a
    /// database or RPC failure, and None if the slot has not been completed yet. If the slot is
    /// completed, an empty list will be returned.
    #[method(name = "get_blobs")]
    async fn get_blobs(&self, blober: Pubkey, slot: u64) -> RpcResult<Option<Vec<Vec<u8>>>>;

    /// Retrieve a proof for a given slot and blober pubkey. Returns an error if there was a
    /// database or RPC failure, and None if the slot has not been completed yet.
    #[method(name = "get_proof")]
    async fn get_proof(&self, blober: Pubkey, slot: u64) -> RpcResult<Option<CompoundProof>>;
}
