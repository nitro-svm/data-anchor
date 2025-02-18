use std::sync::Arc;

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};

use crate::FeeStrategy;

pub mod declare_blob;
pub mod discard_blob;
pub mod finalize_blob;
pub mod insert_chunk;
pub mod set_compute_unit_price;

pub use declare_blob::declare_blob;
pub use discard_blob::discard_blob;
pub use finalize_blob::finalize_blob;
pub use insert_chunk::insert_chunk;

pub struct MessageArguments {
    // The program ID of the blober program.
    pub program_id: Pubkey,
    // The address of the blober account to insert the chunk into.
    pub blober: Pubkey,
    pub payer: Pubkey,
    pub client: Arc<RpcClient>,
    pub fee_strategy: FeeStrategy,
}

impl MessageArguments {
    pub fn new(
        program_id: Pubkey,
        blober: Pubkey,
        payer: &Keypair,
        client: Arc<RpcClient>,
        fee_strategy: FeeStrategy,
    ) -> Self {
        Self {
            client,
            blober,
            program_id,
            fee_strategy,
            payer: payer.pubkey(),
        }
    }
}
