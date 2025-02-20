use anchor_lang::{InstructionData, ToAccountMetas};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
};

use crate::{
    tx::{set_compute_unit_price::set_compute_unit_price, MessageArguments},
    BloberClientResult,
};

pub const COMPUTE_UNIT_LIMIT: u32 = 2_400;

#[allow(dead_code, reason = "Might be used for fee calculation later")]
pub const NUM_SIGNATURES: u16 = 1;

/// Closes a blober account.
pub async fn close_blober(args: &MessageArguments) -> BloberClientResult<Message> {
    let accounts = blober::accounts::Close {
        blober: args.blober,
        payer: args.payer,
    };

    let data = blober::instruction::Close {};

    let instruction = Instruction {
        program_id: args.program_id,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    };

    let set_price =
        set_compute_unit_price(&args.client, &[args.blober, args.payer], args.fee_strategy).await?;
    // This limit is chosen empirically, should blow up in integration tests if it's set too low.
    let set_limit = ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT);

    let msg = Message::new(&[set_price, set_limit, instruction], Some(&args.payer));

    Ok(msg)
}
