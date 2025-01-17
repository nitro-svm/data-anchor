//! This crate contains proofs related to the Solana blockchain.
//!
//! The proofs can prove the state of accounts on the chain and whether or not they were updated,
//! but it makes no semantic assumptions about the account data, it's just considered raw bytes.
//! The account data must first be deserialized and verified that it matches the expected state.

pub mod accounts_delta_hash;
pub mod bank_hash;
pub mod blob;
pub mod blober_account_state;
pub mod compound;
mod debug;
pub mod slot_hash;

#[doc(hidden)]
#[cfg(test)]
pub(crate) mod testing;
