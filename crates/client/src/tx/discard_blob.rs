use anchor_lang::{prelude::Pubkey, InstructionData, ToAccountMetas};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
};

use crate::{
    tx::{set_compute_unit_price::set_compute_unit_price, MessageArguments},
    BloberClientResult,
};

pub const COMPUTE_UNIT_LIMIT: u32 = 40_000;

#[allow(dead_code, reason = "Might be used for fee calculation later")]
pub const NUM_SIGNATURES: u16 = 1;

/// Discards a blob from the blober account.
pub async fn discard_blob(args: &MessageArguments, blob: Pubkey) -> BloberClientResult<Message> {
    let accounts = blober::accounts::DiscardBlob {
        blob,
        blober: args.blober,
        payer: args.payer,
    };

    let data = blober::instruction::DiscardBlob {};

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
