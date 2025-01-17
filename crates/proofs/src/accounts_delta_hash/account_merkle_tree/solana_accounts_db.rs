//! Contains code borrowed from the Solana AccountsDB crate.

use solana_sdk::{
    account::ReadableAccount, blake3, hash::Hash, pubkey::Pubkey, stake_history::Epoch,
};

pub const MERKLE_FANOUT: usize = 16;

/// Source: https://github.com/anza-xyz/agave/blob/v2.0.10/accounts-db/src/accounts_db.rs#L6164-L6173
/// Copied to not pull in the entire AccountsDB crate.
///
/// Not mutation testing Solana code.
#[cfg_attr(test, mutants::skip)]
pub fn hash_account<T: ReadableAccount>(account: &T, pubkey: &Pubkey) -> Hash {
    hash_account_data(
        account.lamports(),
        account.owner(),
        account.executable(),
        account.rent_epoch(),
        account.data(),
        pubkey,
    )
}

/// Source: https://github.com/anza-xyz/agave/blob/v2.0.10/accounts-db/src/accounts_db.rs#L6175-L6218
/// Copied to not pull in the entire AccountsDB crate.
///
/// Not mutation testing Solana code.
#[cfg_attr(test, mutants::skip)]
fn hash_account_data(
    lamports: u64,
    owner: &Pubkey,
    executable: bool,
    rent_epoch: Epoch,
    data: &[u8],
    pubkey: &Pubkey,
) -> Hash {
    if lamports == 0 {
        return Hash::default();
    }
    let mut hasher = blake3::Hasher::default();

    // allocate a buffer on the stack that's big enough to hold a token account or a stake account
    const META_SIZE: usize = 8 /* lamports */ + 8 /* rent_epoch */ + 1 /* executable */ + 32 /* owner */ + 32 /* pubkey */;
    const DATA_SIZE: usize = 200; // stake accounts are 200 B and token accounts are 165-182ish B
    const BUFFER_SIZE: usize = META_SIZE + DATA_SIZE;
    let mut buffer = Vec::with_capacity(BUFFER_SIZE);

    // collect lamports, rent_epoch into buffer to hash
    buffer.extend_from_slice(&lamports.to_le_bytes());
    buffer.extend_from_slice(&rent_epoch.to_le_bytes());

    if data.len() > DATA_SIZE {
        // For larger accounts whose data can't fit into the buffer, update the hash now.
        hasher.hash(&buffer);
        buffer.clear();

        // hash account's data
        hasher.hash(data);
    } else {
        // For small accounts whose data can fit into the buffer, append it to the buffer.
        buffer.extend_from_slice(data);
    }

    // collect exec_flag, owner, pubkey into buffer to hash
    buffer.push(executable.into());
    buffer.extend_from_slice(owner.as_ref());
    buffer.extend_from_slice(pubkey.as_ref());
    hasher.hash(&buffer);

    let bytes: [u8; 32] = hasher.result().as_ref().try_into().unwrap();
    Hash::new_from_array(bytes)
}
