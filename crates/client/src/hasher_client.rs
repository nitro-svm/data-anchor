use std::sync::Arc;

use solana_rpc_client::nonblocking::rpc_client::RpcClient;
pub use solana_rpc_client_api::client_error::Error;
use solana_sdk::{signature::Keypair, signer::Signer, transaction::Transaction};

use crate::{tx, FeeStrategy};

/// A client for the Hasher program. This client can be used to hash the state of an account.
/// It's intended use is that one hasher can be shared between multiple users, by communicating
/// the public key of the hasher account out-of-band. This provides a way to hash the state of
/// an account and store it in a well-known location without having to deal with tracking multiple
/// accounts. It also makes it easy to prove that there is no censorship of transactions by
/// verifying whether the accounts_delta_hash includes the Hasher account or not.
pub struct HasherClient {
    payer: Arc<Keypair>,
    client: Arc<RpcClient>,
}

impl HasherClient {
    /// Creates a new `HasherClient` with the given payer and RPC client.
    pub fn new(payer: Arc<Keypair>, client: Arc<RpcClient>) -> Self {
        Self { payer, client }
    }

    /// Creates a new Hasher account.
    pub async fn create_hasher(
        &self,
        keypair: Option<Keypair>,
        fee_strategy: FeeStrategy,
    ) -> Result<Keypair, Error> {
        let keypair = keypair.unwrap_or_else(Keypair::new);
        let msg = tx::create_hasher(&self.client, &self.payer, &keypair, fee_strategy).await?;
        let recent_blockhash = self.client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new(&[&self.payer, &keypair], msg, recent_blockhash);
        self.client.send_and_confirm_transaction(&tx).await?;
        Ok(keypair)
    }

    /// Closes a Hasher account.
    pub async fn close_hasher(&self, keypair: &Keypair) -> Result<(), Error> {
        let msg = tx::close_hasher(
            &self.client,
            &self.payer,
            keypair.pubkey(),
            FeeStrategy::default(),
        )
        .await?;
        let recent_blockhash = self.client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new(&[&self.payer], msg, recent_blockhash);
        self.client.send_and_confirm_transaction(&tx).await?;
        Ok(())
    }
}
