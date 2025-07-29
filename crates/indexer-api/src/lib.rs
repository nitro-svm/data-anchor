#![doc = include_str!("../README.md")]

mod indexing;
mod rpc;

pub use data_anchor_proofs::compound::CompoundInclusionProof;
pub use indexing::*;
pub use rpc::*;
