#![no_main]
sp1_zkvm::entrypoint!(main);

use data_anchor_prover_core::data_correctness_proof;
use data_anchor_utils::{
    compression::{
        DataAnchorCompression, Default as DefaultCompression, Flate2Compression, Lz4Compression,
        NoCompression, ZstdCompression,
    },
    decompress_and_decode,
    encoding::{Bincode, Borsh, Default as DefaultEncoding, Json, Postcard},
};

fn test_decompress_and_decode(
    compression: &dyn DataAnchorCompression,
    compression_name: &str,
    encoding_name: &str,
    blob: &[u8],
) {
    println!("cycle-tracker-report-start: {encoding_name}_{compression_name}");
    let _decompressed_and_decoded: Result<Vec<u8>, _> = match encoding_name {
        "default_encoding" => decompress_and_decode(&DefaultEncoding, compression, blob),
        "postcard" => decompress_and_decode(&Postcard, compression, blob),
        "bincode" => decompress_and_decode(&Bincode, compression, blob),
        "borsh" => decompress_and_decode(&Borsh, compression, blob),
        "json" => decompress_and_decode(&Json, compression, blob),
        _ => panic!("Unknown encoding: {}", encoding_name),
    };
    println!("cycle-tracker-report-end: {encoding_name}_{compression_name}");
}

fn main() {
    let (_proof, args) = data_correctness_proof();

    let encoding_name: String = sp1_zkvm::io::read();
    let compression_name: String = sp1_zkvm::io::read();

    let blobs = args
        .blobs
        .into_iter()
        .filter_map(|b| b.data)
        .collect::<Vec<_>>();

    for blob in &blobs {
        match compression_name.as_str() {
            "no_compression" => {
                test_decompress_and_decode(&NoCompression, &compression_name, &encoding_name, blob);
            }
            "default_compression" => {
                test_decompress_and_decode(
                    &DefaultCompression,
                    &compression_name,
                    &encoding_name,
                    blob,
                );
            }
            "zstd_compression" => {
                test_decompress_and_decode(
                    &ZstdCompression::default(),
                    &compression_name,
                    &encoding_name,
                    blob,
                );
            }
            "lz4_compression" => {
                test_decompress_and_decode(
                    &Lz4Compression,
                    &compression_name,
                    &encoding_name,
                    blob,
                );
            }
            "flate2_compression" => {
                test_decompress_and_decode(
                    &Flate2Compression,
                    &compression_name,
                    &encoding_name,
                    blob,
                );
            }
            _ => panic!("Unknown compression: {}", compression_name),
        }
    }
}
