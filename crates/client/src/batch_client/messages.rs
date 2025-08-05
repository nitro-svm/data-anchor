use anchor_lang::solana_program::{clock::Slot, hash::Hash};
use solana_signature::Signature;
use solana_transaction::Transaction;
use tokio::sync::mpsc;
use tracing::Span;

use super::transaction::TransactionStatus;

/// Info about the current height of the blockchain.
#[derive(Clone, Debug, Copy, PartialEq, Default)]
pub struct BlockMessage {
    pub blockhash: Hash,
    pub last_valid_block_height: u64,
    pub block_height: u64,
}

/// A transaction that should be sent to the network.
#[derive(Clone, Debug)]
pub struct SendTransactionMessage {
    pub span: Span,
    pub index: usize,
    pub transaction: Transaction,
    pub last_valid_block_height: u64,
    pub response_tx: mpsc::UnboundedSender<StatusMessage>,
}

/// A transaction that has been submitted to the network, and is awaiting confirmation.
#[derive(Clone, Debug)]
pub struct ConfirmTransactionMessage {
    pub span: Span,
    pub index: usize,
    pub transaction: Transaction,
    pub last_valid_block_height: u64,
    pub response_tx: mpsc::UnboundedSender<StatusMessage>,
}

impl From<ConfirmTransactionMessage> for SendTransactionMessage {
    fn from(msg: ConfirmTransactionMessage) -> Self {
        Self {
            span: msg.span,
            index: msg.index,
            transaction: msg.transaction,
            last_valid_block_height: msg.last_valid_block_height,
            response_tx: msg.response_tx,
        }
    }
}

/// A status update for a transaction that has been submitted to the network, good or bad.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusMessage {
    pub index: usize,
    pub landed_as: Option<(Slot, Signature)>,
    pub status: TransactionStatus,
}
