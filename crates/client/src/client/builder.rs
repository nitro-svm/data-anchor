use std::{str::FromStr, sync::Arc};

use data_anchor_utils::{compression::DataAnchorCompressionAsync, encoding::DataAnchorEncoding};
use jsonrpsee::{http_client::HttpClientBuilder, ws_client::HeaderMap};
use solana_cli_config::Config;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;

use crate::{
    BatchClient, DataAnchorClient, DataAnchorClientError, DataAnchorClientResult,
    client::{
        DataAnchorClientBuilder,
        data_anchor_client_builder::{self, IsSet, IsUnset, SetIndexerClient, SetProofClient},
    },
};

impl<Encoding, Compression, State> DataAnchorClientBuilder<Encoding, Compression, State>
where
    Encoding: DataAnchorEncoding + Default,
    Compression: DataAnchorCompressionAsync,
    State: data_anchor_client_builder::State,
{
    /// Adds an indexer client to the builder based on the given indexer URL and optional API token.
    ///
    /// # Example
    ///
    /// ```rust
    /// use data_anchor_client::DataAnchorClient;
    ///
    /// let builder_with_indexer = DataAnchorClient::builder()
    ///     .indexer_from_url("http://localhost:8080", None)
    ///     .await?;
    /// ```
    pub async fn indexer_from_url(
        self,
        indexer_url: &str,
        indexer_api_token: Option<String>,
    ) -> DataAnchorClientResult<
        DataAnchorClientBuilder<Encoding, Compression, SetProofClient<SetIndexerClient<State>>>,
    >
    where
        State::IndexerClient: IsUnset,
        State::ProofClient: IsUnset,
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
        headers.insert(
            "user-agent",
            format!("data-anchor-client/{}", env!("CARGO_PKG_VERSION"))
                .parse()
                .map_err(|_| {
                    DataAnchorClientError::InvalidIndexerApiToken(
                        "Failed to set user-agent".to_owned(),
                    )
                })?,
        );
        let indexer_client = HttpClientBuilder::new()
            .set_headers(headers.clone())
            .build(indexer_url)?;
        let proof_client = HttpClientBuilder::new()
            .set_headers(headers)
            .build(format!("{indexer_url}/proof"))?;
        Ok(self
            .indexer_client(Arc::new(indexer_client))
            .proof_client(Arc::new(proof_client)))
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
    /// use solana_pubkey::Pubkey;
    /// use solana_keypair::Keypair;
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
    ) -> DataAnchorClientResult<DataAnchorClient<Encoding, Compression>>
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
}
