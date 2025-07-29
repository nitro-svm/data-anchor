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
}
