use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Chunk already received")]
    DuplicateChunk,
    #[msg("Invalid public value")]
    InvalidPublicValue,
    #[msg("Blober missmatch in public values")]
    BloberMismatch,
}
