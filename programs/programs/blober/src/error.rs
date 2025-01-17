use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Chunk already received")]
    DuplicateChunk,
}
