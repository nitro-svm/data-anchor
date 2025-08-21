#![no_main]
sp1_zkvm::entrypoint!(main);

use data_anchor_prover_core::data_correctness_proof;
use data_anchor_utils::{
    compression::CompressionType, decompress_and_decode, encoding::EncodingType,
};

fn main() {
    let (_proof, args) = data_correctness_proof();

    let encoding: EncodingType = sp1_zkvm::io::read();
    let compression: CompressionType = sp1_zkvm::io::read();

    let blobs = args
        .blobs
        .into_iter()
        .filter_map(|b| b.data)
        .collect::<Vec<_>>();

    for blob in &blobs {
        println!("cycle-tracker-report-start: {encoding}_{compression}");
        let _decompressed_and_decoded: Result<Vec<u8>, _> = decompress_and_decode(blob);
        println!("cycle-tracker-report-end: {encoding}_{compression}");
    }
}
