use std::collections::BTreeMap;

use anchor_lang::{AnchorSerialize, Discriminator};
use bytesize::ByteSize;
use clap::Parser;
use data_anchor_blober::{
    BLOB_DATA_END, BLOB_DATA_START, CHUNK_SIZE, COMPOUND_TX_SIZE, blob::Blob, initial_hash,
    state::blober::Blober,
};
use data_anchor_proofs::{
    blob::BlobProof,
    blober_account_state::{BlobAccount, BloberAccountStateProof, merge_all_hashes},
    compound::{CompoundInclusionProof, ProofBlob, VerifyArgs},
};
use data_anchor_prover::{DATA_CORRECTNESS_ELF, DAWN_SLA_ELF, run_client};
use rand::{RngCore, rngs::OsRng};
use solana_pubkey::Pubkey;
use sp1_sdk::utils;

#[derive(Debug, Clone, Parser)]
struct Config {
    #[arg(short, long, env = "DATA_ANCHOR_PROVE", default_value_t = false)]
    pub prove: bool,
    #[arg(short, long, env = "DATA_ANCHOR_VERIFY", default_value_t = true)]
    pub verify: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut seed = vec![0u8; 50];
    OsRng.fill_bytes(&mut seed);
    let mut u = arbitrary::Unstructured::new(&seed);

    let slots: u64 = 31 * 3;

    let mut blobs = BTreeMap::<u64, Vec<(ProofBlob<Vec<u8>>, BlobProof, BlobAccount)>>::new();
    for slot in 1..=slots {
        let blob_count: u64 = 1;
        let mut slot_blobs = Vec::with_capacity(blob_count as usize);

        for _ in 0..blob_count {
            // ------------------------- Blob -------------------------
            // MBPS + Total sent + Timestamp + Challenger identity
            let min_blob_size = 8 + 8 + 8 + 32;
            let blob_size = u.int_in_range(min_blob_size..=COMPOUND_TX_SIZE)?;
            let mut blob = vec![0u8; blob_size as usize];
            u.fill_buffer(&mut blob)?;
            if blob.len() > u16::MAX as usize {
                blob = blob[..blob_size as usize].to_vec();
            }

            let mut chunks = blob
                .chunks(CHUNK_SIZE as usize)
                .enumerate()
                .map(|(i, chunk)| (i as u16, chunk))
                .collect::<Vec<_>>();

            // Swap a few chunks around to simulate out-of-order submission.
            for _ in 0..10 {
                let a = u.choose_index(chunks.len())?;
                let b = u.choose_index(chunks.len())?;
                chunks.swap(a, b);
            }

            let blob_address = Pubkey::new_unique();
            let mut blob_state = Blob::new(slot, 0, blob.len() as u32, 0);
            for (chunk_index, chunk_data) in &chunks {
                blob_state.insert(slot, *chunk_index, chunk_data);
            }

            let proof_blob = ProofBlob {
                blob: blob_address,
                data: Some(blob.to_vec()),
            };

            let blob_proof = BlobProof::new(&chunks);

            let blob_account_state = [
                Blob::DISCRIMINATOR.to_vec(),
                blob_state.try_to_vec().unwrap(),
            ]
            .concat()[BLOB_DATA_START..BLOB_DATA_END]
                .to_vec();
            let blob_account = BlobAccount::new(blob_address, blob_account_state);

            slot_blobs.push((proof_blob, blob_proof, blob_account));
        }

        // We want to start insertions at slot 2
        blobs.insert(slot + 1, slot_blobs);
    }

    let blober_pubkey = Pubkey::new_unique();

    let blob_accounts = blobs
        .iter()
        .map(|(slot, accounts)| {
            (
                *slot,
                accounts
                    .iter()
                    .map(|(_, _, account)| account.clone())
                    .collect(),
            )
        })
        .collect();

    let blober_account_state_proof = BloberAccountStateProof::new(initial_hash(), 1, blob_accounts);

    let blob_proofs = blobs
        .values()
        .flat_map(|blobs| {
            blobs
                .iter()
                .map(|(_, proof, _)| proof.clone())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let blob_proof_count = blob_proofs.len();
    let compound_inclusion_proof =
        CompoundInclusionProof::new(blob_proofs, blober_pubkey, blober_account_state_proof);

    let caller = Pubkey::new_unique();
    let namespace = u.arbitrary::<String>()?;

    let hash =
        merge_all_hashes(std::iter::once(initial_hash()).chain(
            blobs.values().flat_map(|slot_blobs| {
                slot_blobs.iter().map(|(_, _, account)| account.hash_blob())
            }),
        ));

    let slot = blobs.keys().max().cloned().unwrap_or(slots + 1);

    let blober_state = Blober {
        caller,
        namespace,
        hash,
        slot,
    };

    let args = VerifyArgs {
        blober: blober_pubkey,
        blober_state: [
            Blober::DISCRIMINATOR,
            blober_state.try_to_vec().unwrap().as_ref(),
        ]
        .concat(),
        blobs: blobs
            .values()
            .flat_map(|blobs| {
                blobs
                    .iter()
                    .map(|(blob, _, _)| blob.clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
    };

    let verification_result =
        compound_inclusion_proof.verify(args.blober, &args.blober_state, args.blobs.as_slice());

    verification_result.unwrap();

    utils::setup_logger();

    let config = Config::parse();

    for (elf, name) in [
        (DATA_CORRECTNESS_ELF, "DATA_CORRECTNESS"),
        (DAWN_SLA_ELF, "DAWN_SLA"),
    ] {
        println!("Running script for {name}");

        let (public_values, report) = run_client(
            &compound_inclusion_proof,
            &args,
            elf,
            config.prove,
            config.verify,
        )?;

        let size = ByteSize(public_values.as_slice().len() as u64);
        println!(
            "{slots},{blob_proof_count},{},{size},{},{}",
            report
                .cycle_tracker
                .iter()
                .map(|(k, v)| format!("{k}:{v}"))
                .collect::<Vec<_>>()
                .join(","),
            report.cycle_tracker.values().sum::<u64>(),
            report.gas.unwrap_or_default(),
        );
    }

    Ok(())
}
