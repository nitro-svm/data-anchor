use anchor_lang::{prelude::Pubkey, InstructionData, ToAccountMetas};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    system_program,
};

use crate::{
    tx::{set_compute_unit_price::set_compute_unit_price, MessageArguments},
    BloberClientResult,
};

pub const COMPUTE_UNIT_LIMIT: u32 = 20_000;

pub const NUM_SIGNATURES: u16 = 1;

/// Declares a blob with the given blober.
pub async fn declare_blob(
    args: &MessageArguments,
    blob: Pubkey,
    timestamp: u64,
    blob_size: u32,
    num_chunks: u16,
) -> BloberClientResult<Message> {
    let accounts = blober::accounts::DeclareBlob {
        blob,
        blober: args.blober,
        payer: args.payer,
        system_program: system_program::id(),
    };

    let data = blober::instruction::DeclareBlob {
        timestamp,
        blob_size,
        num_chunks,
    };

    let instruction = Instruction {
        program_id: args.program_id,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    };

    let set_price =
        set_compute_unit_price(&args.client, &[blob, args.payer], args.fee_strategy).await?;
    // This limit is chosen empirically, should blow up in integration tests if it's set too low.
    let set_limit = ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT);

    let msg = Message::new(&[set_price, set_limit, instruction], Some(&args.payer));

    Ok(msg)
}
