#![no_main]
sp1_zkvm::entrypoint!(main);

use data_anchor_prover_core::data_correctness_proof;

fn main() {
    data_correctness_proof();
}
