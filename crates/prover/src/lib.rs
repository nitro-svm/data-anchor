use data_anchor_proofs::compound::{CompoundInclusionProof, VerifyArgs};
use sp1_sdk::{ExecutionReport, ProverClient, SP1PublicValues, SP1Stdin, include_elf};

const ELF: &[u8] = include_elf!("data-anchor-proof-program");

pub fn generate_proof(
    compound_inclusion_proof: CompoundInclusionProof,
    args: VerifyArgs,
    prove: bool,
    verify: bool,
) -> Result<(SP1PublicValues, ExecutionReport), Box<dyn std::error::Error>> {
    let mut sp1_stdin = SP1Stdin::new();
    sp1_stdin.write(&compound_inclusion_proof);
    sp1_stdin.write(&args);

    let client = ProverClient::from_env();
    let (pk, vk) = client.setup(ELF);

    let (public_values, report) = client.execute(ELF, &sp1_stdin).run()?;

    if prove {
        let proof = client.prove(&pk, &sp1_stdin).groth16().run()?;

        println!("Proof bytes: {}", proof.bytes().len());

        if verify {
            match client.verify(&proof, &vk) {
                Ok(_) => {
                    println!("Proof is valid.");
                }
                Err(e) => {
                    eprintln!("Error verifying proof: {e}");
                }
            }
        }
    }

    Ok((public_values, report))
}
