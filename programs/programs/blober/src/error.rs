use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Chunk already received")]
    DuplicateChunk,
    #[msg("Invalid public value")]
    InvalidPublicValue,
    #[msg("Blober missmatch in public values")]
    BloberMismatch,
    #[msg("New proof initial hash does not match previous final hash")]
    ProofHashMismatch,
    #[msg("New proof slot must be greater than previous slot")]
    SlotTooLow,
    #[msg("Only verifier programs can update checkpoints")]
    InvalidInstructionProgramId,
    #[msg("Public values exceed maximum size")]
    PublicValuesExceedMaxSize,
    #[msg("Invalid verification key size")]
    InvalidVerificationKeySize,
    #[msg("Proof verification failed")]
    ProofVerificationFailed,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Checkpoint not up to date with blober state")]
    CheckpointNotUpToDate,
    #[msg("Checkpoint config not found for existing checkpoint")]
    CheckpointWithoutConfig,
    #[msg("Blob is missing chunks, can't be completed in this state")]
    BlobNotComplete,
}
