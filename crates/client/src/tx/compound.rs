use blober::instruction::{DeclareBlob, FinalizeBlob, InsertChunk};
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};

use crate::tx::{MessageArguments, MessageBuilder};

pub struct Compound {
    declare: DeclareBlob,
    insert: InsertChunk,
    blob: Pubkey,
}

impl Compound {
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

impl From<&Compound> for <DeclareBlob as MessageBuilder>::Input {
    fn from(value: &Compound) -> Self {
        (
            DeclareBlob {
                timestamp: value.declare.timestamp,
                blob_size: value.declare.blob_size,
            },
            value.blob,
        )
    }
}

impl From<&Compound> for <InsertChunk as MessageBuilder>::Input {
    fn from(value: &Compound) -> Self {
        (
            InsertChunk {
                idx: value.insert.idx,
                data: value.insert.data.clone(),
            },
            value.blob,
        )
    }
}

impl From<&Compound> for <FinalizeBlob as MessageBuilder>::Input {
    fn from(value: &Compound) -> Self {
        value.blob
    }
}

impl MessageBuilder for Compound {
    type Input = Self;
    const COMPUTE_UNIT_LIMIT: u32 = DeclareBlob::COMPUTE_UNIT_LIMIT
        + InsertChunk::COMPUTE_UNIT_LIMIT
        + FinalizeBlob::COMPUTE_UNIT_LIMIT;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.input.blob, args.blober, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        [
            DeclareBlob::generate_instructions(&args.to_other()),
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
        let data: [u8; blober::COMPOUND_TX_SIZE as usize] = u.arbitrary()?;
        let blob = blober::find_blob_address(blober::id(), payer, blober, timestamp, data.len());

        Ok(Compound::new(blob, timestamp, data.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use crate::tx::{Compound, MessageBuilder};

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        Compound::test_compute_unit_limit();
    }
}
