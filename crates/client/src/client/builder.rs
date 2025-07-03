use std::{str::FromStr, sync::Arc};

use jsonrpsee::{http_client::HttpClientBuilder, ws_client::HeaderMap};
use solana_cli_config::Config;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;

use crate::{
    BatchClient, DataAnchorClient, DataAnchorClientError, DataAnchorClientResult,
    client::{
        DataAnchorClientBuilder,
        data_anchor_client_builder::{
            self, IsSet, IsUnset, SetHeliusFeeEstimate, SetIndexerClient,
        },
    },
};

impl<State: data_anchor_client_builder::State> DataAnchorClientBuilder<State> {
    /// Adds an indexer client to the builder based on the given indexer URL.
    ///
    /// # Example
    ///
    /// ```rust
    /// use data_anchor_client::DataAnchorClient;
    ///
    /// let builder_with_indexer = DataAnchorClient::builder()
    ///     .indexer_from_url("ws://localhost:8080")
    ///     .await?;
    /// ```
    pub async fn indexer_from_url(
        self,
        indexer_url: &str,
        indexer_api_token: Option<String>,
    ) -> DataAnchorClientResult<DataAnchorClientBuilder<SetIndexerClient<State>>>
    where
        State::IndexerClient: IsUnset,
    {
        let mut headers = HeaderMap::new();
        if let Some(token) = indexer_api_token {
            headers.insert(
                "x-api-key",
                token.parse().map_err(|_| {
                    DataAnchorClientError::InvalidIndexerApiToken(
                        "Failed to parse API token".to_owned(),
                    )
                })?,
            );
        }
        let indexer_client = HttpClientBuilder::new()
            .set_headers(headers)
            .build(indexer_url)?;
        Ok(self.indexer_client(Arc::new(indexer_client)))
    }

    /// Builds a new `DataAnchorClient` with an RPC client and a batch client built from the given
    /// Solana cli [`Config`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::sync::Arc;
    ///
    /// use data_anchor_client::{DataAnchorClient};
    /// use solana_cli_config::Config;
    /// use solana_sdk::{pubkey::Pubkey, signature::Keypair};
    ///
    /// let payer = Arc::new(Keypair::new());
    /// let program_id = Pubkey::new_unique();
    /// let solana_config = Config::default();
    /// let client = DataAnchorClient::builder()
    ///     .payer(payer)
    ///     .program_id(program_id)
    ///     .build_with_config(solana_config)
    ///     .await?;
    /// ```
    pub async fn build_with_config(
        self,
        solana_config: Config,
    ) -> DataAnchorClientResult<DataAnchorClient>
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

    pub fn with_helius_fee_estimate(self) -> DataAnchorClientBuilder<SetHeliusFeeEstimate<State>>
    where
        State::HeliusFeeEstimate: IsUnset,
    {
        self.helius_fee_estimate(true)
    }
}
