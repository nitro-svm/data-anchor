#![doc = include_str!("../README.md")]

use data_anchor_proofs::compound::{
    completeness::CompoundCompletenessProof, inclusion::CompoundInclusionProof,
};
use serde::{Deserialize, Serialize};

mod indexing;
mod rpc;

pub use indexing::*;
pub use rpc::*;

/// A compound proof that proves whether a blob has been published in a specific slot.
/// See [`CompoundInclusionProof`] and [`CompoundCompletenessProof`] for more information.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum CompoundProof {
    /// See [`CompoundInclusionProof`].
    Inclusion(CompoundInclusionProof),
    /// See [`CompoundCompletenessProof`].
    Completeness(CompoundCompletenessProof),
}

impl CompoundProof {
    /// Returns if the proof is an inclusion proof.
    pub fn is_inclusion(&self) -> bool {
        matches!(self, CompoundProof::Inclusion(_))
    }
}
