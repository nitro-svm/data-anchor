use data_anchor_api::ProofData;
use data_anchor_proofs::compound::{CompoundInclusionProof, VerifyArgs};
use sp1_sdk::{
    ExecutionReport, HashableKey, ProverClient, SP1PublicValues, SP1Stdin, SP1VerificationError,
    include_elf,
};
use tokio::task::spawn_blocking;
use tracing::{debug, info};

const ELF: &[u8] = include_elf!("data-anchor-proof-program");

#[derive(Debug, thiserror::Error)]
pub enum ProofGenerationError {
    #[error("Failed to run generation task: {0}")]
    RunGenerationTask(#[from] tokio::task::JoinError),
    #[error("Failed to generate proof: {0}")]
    Generate(#[from] anyhow::Error),
    #[error("Failed to verify proof: {0}")]
    Verify(#[from] SP1VerificationError),
}

pub type ProofGenerationResult<T = ()> = Result<T, ProofGenerationError>;

#[tracing::instrument(level = "info", skip_all, fields(slot = compound_inclusion_proof.target_slot(), blober = %args.blober))]
pub fn simulate_proof_generation(
    compound_inclusion_proof: CompoundInclusionProof,
    args: VerifyArgs,
) -> ProofGenerationResult<(SP1PublicValues, ExecutionReport)> {
    let mut sp1_stdin = SP1Stdin::new();
    sp1_stdin.write(&compound_inclusion_proof);
    sp1_stdin.write(&args);

    let client = ProverClient::from_env();

    debug!("Simulating proof generation");
    let (public_values, report) = client.execute(ELF, &sp1_stdin).run()?;

    Ok((public_values, report))
}

pub fn run_client(
    compound_inclusion_proof: CompoundInclusionProof,
    args: VerifyArgs,
    prove: bool,
    verify: bool,
) -> ProofGenerationResult<(SP1PublicValues, ExecutionReport)> {
    let mut sp1_stdin = SP1Stdin::new();
    sp1_stdin.write(&compound_inclusion_proof);
    sp1_stdin.write(&args);

    let client = ProverClient::from_env();

    if prove {
        debug!("Generating Groth16 proof");
        let (pk, vk) = client.setup(ELF);
        let proof = client.prove(&pk, &sp1_stdin).groth16().run()?;

        if verify {
            debug!("Verifying Groth16 proof");
            client.verify(&proof, &vk)?;
        }
    }

    debug!("Executing SP1 program");
    let (public_values, report) = client.execute(ELF, &sp1_stdin).run()?;

    Ok((public_values, report))
}

#[tracing::instrument(level = "info", skip_all, fields(slot = compound_inclusion_proof.target_slot(), blober = %args.blober))]
pub async fn generate_proof(
    compound_inclusion_proof: CompoundInclusionProof,
    args: VerifyArgs,
) -> ProofGenerationResult<ProofData> {
    let mut sp1_stdin = SP1Stdin::new();
    sp1_stdin.write(&compound_inclusion_proof);
    sp1_stdin.write(&args);

    let client = ProverClient::from_env();
    let (pk, vk) = client.setup(ELF);

    info!("Generating Groth16 proof");
    let proof = spawn_blocking(move || client.prove(&pk, &sp1_stdin).groth16().run()).await??;

    Ok(ProofData {
        proof: proof.bytes().to_vec(),
        public_values: proof.public_values.to_vec(),
        verification_key: vk.bytes32(),
    })
}
