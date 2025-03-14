use anchor_lang::solana_program::hash::hashv;
use rand::{prelude::SliceRandom, thread_rng};

use crate::{blob::Blob, compute_blob_digest, initial_hash, CHUNK_SIZE};

fn test(blob: Vec<u8>) {
    println!(
        "Running test with blob size {} ({} chunks)",
        blob.len(),
        blob.len().div_ceil(CHUNK_SIZE as usize)
    );

    let mut blober = Blob::new(
        0,
        0,
        blob.len() as u32,
        blob.len().div_ceil(CHUNK_SIZE as usize) as u16,
        0,
    );

    let mut chunks = blob
        .chunks(CHUNK_SIZE as usize)
        .enumerate()
        .map(|(i, chunk)| (i as u16, chunk))
        .collect::<Vec<_>>();
    chunks.shuffle(&mut thread_rng());

    for (i, chunk) in &chunks {
        blober.insert(0, *i, chunk);
    }

    let expected_blob_digest = compute_blob_digest(&chunks);

    assert_eq!(&expected_blob_digest, blober.blob_digest());
}

#[test]
fn specific_blob_sizes() {
    let test_cases = [
        // Empty blob.
        Vec::new(),
        // One byte.
        vec![0u8; 1],
        // One byte short of a chunk.
        vec![0u8; CHUNK_SIZE as usize - 1],
        // Exactly one chunk.
        vec![0u8; CHUNK_SIZE as usize],
        // One chunk and one byte.
        vec![0u8; CHUNK_SIZE as usize + 1],
        // One byte short of five chunks.
        vec![0u8; CHUNK_SIZE as usize - 1],
        // Exactly five chunks.
        vec![0u8; CHUNK_SIZE as usize],
        // Five chunks and one byte.
        vec![0u8; CHUNK_SIZE as usize + 1],
    ];

    for blob in test_cases {
        test(blob);
    }
}

#[test]
fn test_blob() {
    let mut acc = Blob::new(0, 0, CHUNK_SIZE as u32 * 5, 5, 0);

    let mut current_digest = initial_hash();
    assert_eq!(current_digest, acc.digest);
    acc.insert(0, 0, &[0u8; CHUNK_SIZE as usize]);
    current_digest = hashv(&[
        &current_digest,
        0_u16.to_le_bytes().as_ref(),
        &[0u8; CHUNK_SIZE as usize],
    ])
    .to_bytes();
    assert_eq!(current_digest, acc.digest);

    acc.insert(0, 2, &[2u8; CHUNK_SIZE as usize]);
    current_digest = hashv(&[
        &current_digest,
        2_u16.to_le_bytes().as_ref(),
        &[2u8; CHUNK_SIZE as usize],
    ])
    .to_bytes();
    assert_eq!(current_digest, acc.digest);

    acc.insert(0, 3, &[3u8; CHUNK_SIZE as usize]);
    current_digest = hashv(&[
        &current_digest,
        3_u16.to_le_bytes().as_ref(),
        &[3u8; CHUNK_SIZE as usize],
    ])
    .to_bytes();
    assert_eq!(current_digest, acc.digest);

    acc.insert(0, 1, &[1u8; CHUNK_SIZE as usize]);
    current_digest = hashv(&[
        &current_digest,
        1_u16.to_le_bytes().as_ref(),
        &[1u8; CHUNK_SIZE as usize],
    ])
    .to_bytes();
    assert_eq!(current_digest, acc.digest);

    acc.insert(0, 4, &[4u8; CHUNK_SIZE as usize]);
    current_digest = hashv(&[
        &current_digest,
        4_u16.to_le_bytes().as_ref(),
        &[4u8; CHUNK_SIZE as usize],
    ])
    .to_bytes();
    assert_eq!(current_digest, acc.digest);
    assert_eq!(acc.blob_digest(), &current_digest);
}
