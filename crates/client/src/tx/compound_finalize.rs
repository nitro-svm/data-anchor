use data_anchor_blober::instruction::{FinalizeBlob, InsertChunk};
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};

use crate::{
    TransactionType,
    tx::{MessageArguments, MessageBuilder},
};

pub struct CompoundFinalize {
    insert: InsertChunk,
    blob: Pubkey,
}

impl CompoundFinalize {
    pub fn new(idx: u16, data: Vec<u8>, blob: Pubkey) -> Self {
        Self {
            insert: InsertChunk { idx, data },
            blob,
        }
    }
}

impl From<&CompoundFinalize> for <InsertChunk as MessageBuilder>::Input {
    fn from(value: &CompoundFinalize) -> Self {
        (
            InsertChunk {
                idx: value.insert.idx,
                data: value.insert.data.clone(),
            },
            value.blob,
        )
    }
}

impl From<&CompoundFinalize> for <FinalizeBlob as MessageBuilder>::Input {
    fn from(value: &CompoundFinalize) -> Self {
        value.blob
    }
}

impl MessageBuilder for CompoundFinalize {
    type Input = Self;
    const TX_TYPE: TransactionType = TransactionType::CompoundFinalize;
    const COMPUTE_UNIT_LIMIT: u32 =
        InsertChunk::COMPUTE_UNIT_LIMIT + FinalizeBlob::COMPUTE_UNIT_LIMIT;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.input.blob, args.blober, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        [
            InsertChunk::generate_instructions(&args.to_other()),
            FinalizeBlob::generate_instructions(&args.to_other()),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    #[cfg(test)]
    fn generate_arbitrary_input(
        u: &mut arbitrary::Unstructured,
        payer: Pubkey,
        blober: Pubkey,
    ) -> arbitrary::Result<Self::Input> {
        let timestamp: u64 = u.arbitrary()?;
        let chunk_idx: u16 = u.arbitrary()?;
        let chunk_data: Vec<u8> = u.arbitrary()?;
        let blob_size: usize = u.arbitrary()?;
        let blob = data_anchor_blober::find_blob_address(
            data_anchor_blober::id(),
            payer,
            blober,
            timestamp,
            blob_size,
        );

        Ok(CompoundFinalize {
            insert: InsertChunk {
                idx: chunk_idx,
                data: chunk_data.clone(),
            },
            blob,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::tx::{CompoundFinalize, MessageBuilder};

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        CompoundFinalize::test_compute_unit_limit();
    }
}
