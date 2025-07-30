use data_anchor_proofs::compound::{CompoundInclusionProof, VerifyArgs};

#[sp1_derive::cycle_tracker]
fn read_data_correctness_inputs_inner() -> (CompoundInclusionProof, VerifyArgs) {
    let proof: CompoundInclusionProof = sp1_zkvm::io::read();
    let args: VerifyArgs = sp1_zkvm::io::read();

    (proof, args)
}

#[sp1_derive::cycle_tracker]
fn data_correctness_commitment_inner(proof: &CompoundInclusionProof, args: &VerifyArgs) {
    sp1_zkvm::io::commit(&proof.blober_pubkey);
    sp1_zkvm::io::commit(&proof.into_commitment());
    sp1_zkvm::io::commit(&args.into_commitment().unwrap());
}

#[sp1_derive::cycle_tracker]
fn verify_data_correctness_inner(proof: &CompoundInclusionProof, args: &VerifyArgs) {
    proof
        .verify(args.blober, &args.blober_state, &args.blobs)
        .unwrap();
}

#[sp1_derive::cycle_tracker]
fn data_correctness_proof_inner() -> (CompoundInclusionProof, VerifyArgs) {
    let (proof, args) = read_data_correctness_inputs();
    data_correctness_commitment(&proof, &args);
    verify_data_correctness(&proof, &args);
    (proof, args)
}

/// Read the prover inputs needed for the data correctness proof.
pub fn read_data_correctness_inputs() -> (CompoundInclusionProof, VerifyArgs) {
    println!("cycle-tracker-report-start: read_data_correctness_inputs_inner");
    let inputs = read_data_correctness_inputs_inner();
    println!("cycle-tracker-report-end: read_data_correctness_inputs_inner");
    inputs
}

/// Commit to the inputs for the data correctness proof. This includes the blober public key,
/// initial state of the blober hash and the final state of the hash.
///
/// # Panics
///
/// This function will panic if the blober state bytes cannot be deserialized into a [`Blober`]
/// state PDA.
pub fn data_correctness_commitment(proof: &CompoundInclusionProof, args: &VerifyArgs) {
    println!("cycle-tracker-report-start: data_correctness_commitment_inner");
    data_correctness_commitment_inner(proof, args);
    println!("cycle-tracker-report-end: data_correctness_commitment_inner");
}

/// Run the verification of the data correctness proof. For more details see the
/// [`CompoundInclusionProof`].
///
/// # Panics
///
/// This function will panic if the proof verification fails.
pub fn verify_data_correctness(proof: &CompoundInclusionProof, args: &VerifyArgs) {
    println!("cycle-tracker-report-start: verify_data_correctness_inner");
    verify_data_correctness_inner(proof, args);
    println!("cycle-tracker-report-end: verify_data_correctness_inner");
}

/// Run the data correctness proof. This is a wrapper around the [`read_data_correctness_inputs`],
/// [`data_correctness_commitment`] and [`verify_data_correctness`] functions and returns the [`CompoundInclusionProof`] and [`VerifyArgs`].
///
/// We first read the data correctness required inputs (the compound inclusion proof and the
/// verification args), then commit to the public values (the blober public key, initial state of
/// the blober hash and the final state of the hash) and finally verify the proof.
///
/// # Panics
///
/// This function will panic if committing to values due to not being able to deserialize the
/// blober state or the proof verification failing.
pub fn data_correctness_proof() -> (CompoundInclusionProof, VerifyArgs) {
    println!("cycle-tracker-report-start: data_correctness_proof_inner");
    let inputs = data_correctness_proof_inner();
    println!("cycle-tracker-report-end: data_correctness_proof_inner");
    inputs
}
