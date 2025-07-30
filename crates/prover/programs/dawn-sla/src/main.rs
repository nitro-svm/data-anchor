#![no_main]
sp1_zkvm::entrypoint!(main);

use data_anchor_prover_core::data_correctness_proof;

fn get_sla_from_blob_data(data: &[u8]) -> u64 {
    let sla_bytes: [u8; 8] = data[..8].try_into().unwrap();
    u64::from_le_bytes(sla_bytes)
}

fn get_sla_score(blobs: &[&[u8]]) -> f64 {
    let sla_sum = blobs
        .iter()
        .map(|&data| get_sla_from_blob_data(data))
        .sum::<u64>();
    sla_sum as f64 / blobs.len() as f64
}

fn main() {
    let (_proof, args) = data_correctness_proof();

    println!("cycle-tracker-report-start: dawn_sla");
    let blob_data = args
        .blobs
        .iter()
        .map(|b| b.data.as_ref().unwrap().as_slice())
        .collect::<Vec<_>>();
    let sla_score = get_sla_score(&blob_data);

    sp1_zkvm::io::commit(&sla_score);
    println!("cycle-tracker-report-end: dawn_sla");
}
