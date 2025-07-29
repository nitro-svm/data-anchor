// These two lines are necessary for the program to properly compile.
//
// Under the hood, we wrap your main function with some extra code so that it behaves properly
// inside the zkVM.
#![no_main]
sp1_zkvm::entrypoint!(main);

use data_anchor_proofs::compound::{CompoundInclusionProof, VerifyArgs};

fn main() {
    // Read the arguments from the input stream.
    println!("cycle-tracker-report-start: read");
    let proof: CompoundInclusionProof = sp1_zkvm::io::read();
    let args: VerifyArgs = sp1_zkvm::io::read();
    println!("cycle-tracker-report-end: read");

    // Commit to inputs
    println!("cycle-tracker-report-start: commit");
    sp1_zkvm::io::commit(&proof.into_commitment());
    sp1_zkvm::io::commit(&args.into_commitment().unwrap());
    println!("cycle-tracker-report-end: commit");

    // Verify the proof
    println!("cycle-tracker-report-start: verify");
    proof
        .verify(args.blober, &args.blober_state, &args.blobs)
        .unwrap();
    println!("cycle-tracker-report-end: verify");
}
