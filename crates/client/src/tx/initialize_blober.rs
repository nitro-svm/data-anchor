use anchor_lang::{InstructionData, ToAccountMetas};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    system_program,
};

use crate::{
    tx::{set_compute_unit_price::set_compute_unit_price, MessageArguments},
    BloberClientResult,
};

pub const COMPUTE_UNIT_LIMIT: u32 = 15_000;

#[allow(dead_code, reason = "Might be used for fee calculation later")]
pub const NUM_SIGNATURES: u16 = 1;

/// Initializes the blober account with a given trusted caller set as the payer of this transaction.
pub async fn initialize_blober(
    args: &MessageArguments,
    namespace: String,
) -> BloberClientResult<Message> {
    let accounts = blober::accounts::Initialize {
        blober: args.blober,
        payer: args.payer,
        system_program: system_program::id(),
    };

    let data = blober::instruction::Initialize {
        namespace,
        trusted: args.payer,
    };

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
