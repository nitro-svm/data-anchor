use std::{str::FromStr, sync::Arc};

use jsonrpsee::ws_client::WsClientBuilder;
use solana_cli_config::Config;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;

use crate::{
    blober_client::{
        blober_client_builder::{self, IsSet, IsUnset, SetHeliusFeeEstimate, SetIndexerClient},
        BloberClientBuilder,
    },
    BatchClient, BloberClient, BloberClientResult,
};

impl<State: blober_client_builder::State> BloberClientBuilder<State> {
    /// Adds an indexer client to the builder based on the given indexer URL.
    ///
    /// # Example
    ///
    /// ```rust
    /// use blober_client::BloberClient;
    ///
    /// let builder_with_indexer = BloberClient::builder()
    ///     .indexer_from_url("ws://localhost:8080")
    ///     .await?;
    /// ```
    pub async fn indexer_from_url(
        self,
        indexer_url: &str,
    ) -> BloberClientResult<BloberClientBuilder<SetIndexerClient<State>>>
    where
        State::IndexerClient: IsUnset,
    {
        let indexer_client = WsClientBuilder::new().build(indexer_url).await?;
        Ok(self.indexer_client(Arc::new(indexer_client)))
    }

    /// Builds a new `BloberClient` with an RPC client and a batch client built from the given
    /// Solana cli [`Config`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::sync::Arc;
    ///
    /// use blober_client::{BloberClient};
    /// use solana_cli_config::Config;
    /// use solana_sdk::{pubkey::Pubkey, signature::Keypair};
    ///
    /// let payer = Arc::new(Keypair::new());
    /// let program_id = Pubkey::new_unique();
    /// let solana_config = Config::default();
    /// let client = BloberClient::builder()
    ///     .payer(payer)
    ///     .program_id(program_id)
    ///     .build_with_config(solana_config)
    ///     .await?;
    /// ```
    pub async fn build_with_config(self, solana_config: Config) -> BloberClientResult<BloberClient>
    where
        State::Payer: IsSet,
        State::ProgramId: IsSet,
        State::RpcClient: IsUnset,
        State::BatchClient: IsUnset,
    {
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            solana_config.json_rpc_url.clone(),
            CommitmentConfig::from_str(&solana_config.commitment)?,
        ));
        let payer = self.get_payer().clone();
        Ok(self
            .rpc_client(rpc_client.clone())
            .batch_client(BatchClient::new(rpc_client.clone(), vec![payer.clone()]).await?)
            .build())
    }

    pub fn with_helius_fee_estimate(self) -> BloberClientBuilder<SetHeliusFeeEstimate<State>>
    where
        State::HeliusFeeEstimate: IsUnset,
    {
        self.helius_fee_estimate(true)
    }
}
