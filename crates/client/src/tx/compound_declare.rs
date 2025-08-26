use anchor_lang::{
    Discriminator, Space, prelude::Pubkey, solana_program::instruction::Instruction,
};
use data_anchor_blober::{
    blob::Blob,
    instruction::{DeclareBlob, InsertChunk},
    state::blober::Blober,
};

use crate::{
    TransactionType,
    tx::{MessageArguments, MessageBuilder},
};

pub struct CompoundDeclare {
    pub declare: DeclareBlob,
    pub insert: InsertChunk,
    pub blob: Pubkey,
}

impl CompoundDeclare {
    pub(crate) fn new(blob: Pubkey, timestamp: u64, blob_data: Vec<u8>) -> Self {
        Self {
            declare: DeclareBlob {
                timestamp,
                blob_size: blob_data.len() as u32,
            },
            insert: InsertChunk {
                idx: 0,
                data: blob_data,
            },
            blob,
        }
    }
}

impl From<&CompoundDeclare> for <DeclareBlob as MessageBuilder>::Input {
    fn from(value: &CompoundDeclare) -> Self {
        (
            DeclareBlob {
                timestamp: value.declare.timestamp,
                blob_size: value.declare.blob_size,
            },
            value.blob,
        )
    }
}

impl From<&CompoundDeclare> for <InsertChunk as MessageBuilder>::Input {
    fn from(value: &CompoundDeclare) -> Self {
        (
            InsertChunk {
                idx: value.insert.idx,
                data: value.insert.data.clone(),
            },
            value.blob,
        )
    }
}

impl MessageBuilder for CompoundDeclare {
    type Input = Self;
    const TX_TYPE: TransactionType = TransactionType::CompoundDeclare;
    const COMPUTE_UNIT_LIMIT: u32 =
        DeclareBlob::COMPUTE_UNIT_LIMIT + InsertChunk::COMPUTE_UNIT_LIMIT;
    const LOADED_ACCOUNT_DATA_SIZE: u32 = (Blober::DISCRIMINATOR.len()
        + Blober::INIT_SPACE
        + Blob::DISCRIMINATOR.len()
        + Blob::INIT_SPACE) as u32;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.input.blob, args.blober, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        [
            DeclareBlob::generate_instructions(&args.to_other()),
            InsertChunk::generate_instructions(&args.to_other()),
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
        let blob_data: [u8; data_anchor_blober::COMPOUND_DECLARE_TX_SIZE as usize] =
            u.arbitrary()?;
        let blob = data_anchor_blober::find_blob_address(
            data_anchor_blober::id(),
            payer,
            blober,
            timestamp,
            blob_data.len(),
        );

        Ok(CompoundDeclare {
            declare: DeclareBlob {
                timestamp,
                blob_size: blob_data.len() as u32,
            },
            insert: InsertChunk {
                idx: 0,
                data: blob_data.to_vec(),
            },
            blob,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::tx::{CompoundDeclare, MessageBuilder};

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        CompoundDeclare::test_compute_unit_limit();
    }
}
