use anchor_lang::{prelude::*, solana_program::hash, Discriminator};

use crate::blob::Blob;

/// The seed used to derive the PDA address of each blob.
#[constant]
pub const SEED: &[u8] = b"blobs";

/// The seed used to derive the PDA address of each checkpoint.
#[constant]
pub const CHECKPOINT_SEED: &[u8] = b"checkpoint";

/// The seed used to derive the PDA address of each checkpoint's config.
#[constant]
pub const CHECKPOINT_CONFIG_SEED: &[u8] = b"config";

/// The seed used to derive the PDA signer for checkpoint creation.
#[constant]
pub const CHECKPOINT_PDA_SIGNER_SEED: &[u8] = b"signer";

/// The maximum number of chunks a blob can occupy.
#[constant]
pub const MAX_CHUNKS: u16 = 2048;

/// The maximum size of a blob.
#[constant]
pub const MAX_BLOB_SIZE: u32 = MAX_CHUNKS as u32 * CHUNK_SIZE as u32;

/// The size of a chunk in bytes. Blobs larger than this must be split into chunks of at most this size.
#[constant]
pub const CHUNK_SIZE: u16 = 915;

/// The maximum length of a namespace string.
#[constant]
pub const MAX_NAMESPACE_LENGTH: u8 = 100;

/// The max size of data for a compound transaction containing all three (declare, insert and finalize) instructions.
pub const COMPOUND_TX_SIZE: u16 = 848;

/// The max size of data for a compound transaction containing the first two (declare and insert) instructions.
pub const COMPOUND_DECLARE_TX_SIZE: u16 = 868;

/// The index of the blob account in the instruction accounts list.
pub const BLOB_ACCOUNT_INSTRUCTION_IDX: usize = 0;

/// The index of the blober account in the instruction accounts list.
pub const BLOB_BLOBER_INSTRUCTION_IDX: usize = 1;

/// The index of the payer account in the instruction accounts list.
pub const BLOB_PAYER_INSTRUCTION_IDX: usize = 2;

/// The size (in bytes) of the bitmap needed to track which chunks have arrived
#[constant]
pub const CHUNKS_BITMAP_SIZE: u16 = MAX_CHUNKS / 8;

/// The maximum number of slots between incremental digest updates. Counting on a 500ms slot time,
/// this is roughly 5 minutes.
#[constant]
pub const BLOB_SLOT_INCREMENTAL_DELAY_LIMIT: u64 = 5 * 60 * 2;

/// The maximum number of slots between the first and last digest updates. Counting on a 500ms slot time,
/// this is roughly 15 minutes.
#[constant]
pub const BLOB_SLOT_TOTAL_DELAY_LIMIT: u64 = 15 * 60 * 2;

/// The offset of the account data in a blob account. This is the start of the relevant data.
#[constant]
pub const BLOB_ACCOUNT_DATA_OFFSET: u8 = Blob::DISCRIMINATOR.len() as u8;

pub const U32_SIZE_BYTES: u8 = core::mem::size_of::<u32>() as u8;

/// The size of the relevant data for a blob account.
#[constant]
pub const BLOB_ACCOUNT_DATA_LEN: u8 = hash::HASH_BYTES as u8 + U32_SIZE_BYTES;

/// The start of the blob data in the account data.
pub const BLOB_DATA_START: usize = BLOB_ACCOUNT_DATA_OFFSET as usize;
/// The end of the blob data in the account data.
pub const BLOB_DATA_END: usize = BLOB_DATA_START + BLOB_ACCOUNT_DATA_LEN as usize;

/// The initial hash value for the blob digest.
pub fn initial_hash() -> [u8; hash::HASH_BYTES] {
    hash::Hasher::default().result().to_bytes()
}

/// The size of a Groth16 proof in bytes.
pub const GROTH16_PROOF_SIZE: usize = 260;

/// The size of a proof public values in bytes.
pub const PROOF_PUBLIC_VALUES_MAX_SIZE: usize = 104;

/// The size of a proof verification key in bytes.
pub const PROOF_VERIFICATION_KEY_SIZE: usize = 32 /* hash::HASH_BYTES */ * 2 /* hex encoding */ + 2 /* "0x" prefix */;
