/// Default number of concurrent requests to send to the RPC.
pub const DEFAULT_CONCURRENCY: usize = 100;

/// Default number of slots to look back for the
/// [`crate::client::DataAnchorClient::get_ledger_blobs`] method.
pub const DEFAULT_LOOKBACK_SLOTS: u64 = 100;

const MAINNET_GENESIS_HASH: &str = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvDPxV6zKj1rS1n";
const DEVNET_GENESIS_HASH: &str = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";
const TESTNET_GENESIS_HASH: &str = "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY";

/// Error types for the indexer URL handling.
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum IndexerUrlError {
    /// Failed to parse the indexer URL.
    #[error("Failed to parse indexer URL: {0}")]
    InvalidUrl(String),

    /// The indexer URL is not supported for the given genesis hash.
    #[error("Unknown genesis hash: {0}")]
    UnknownGenesisHash(String),

    /// The indexer URL is not supported for the Testnet.
    #[error("Testnet is not supported")]
    TestnetNotSupported,
}

/// Result type for operations involving the indexer URL.
pub type IndexerUrlResult<T = ()> = Result<T, IndexerUrlError>;

/// Default Indexer API URL assignments.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum IndexerUrl {
    Staging,
    #[default]
    Devnet,
    Mainnet,
    Custom(String),
}

impl IndexerUrl {
    /// Returns the indexer URL based on the given Solana genesis hash.
    pub fn from_genesis_hash(genesis_hash: &str) -> IndexerUrlResult<Self> {
        match genesis_hash {
            MAINNET_GENESIS_HASH => Ok(IndexerUrl::Mainnet),
            DEVNET_GENESIS_HASH => Ok(IndexerUrl::Devnet),
            TESTNET_GENESIS_HASH => Err(IndexerUrlError::TestnetNotSupported),
            _ => Err(IndexerUrlError::UnknownGenesisHash(
                genesis_hash.to_string(),
            )),
        }
    }

    /// Returns the default URL for the given indexer environment.
    pub fn url(&self) -> String {
        let name = match self {
            IndexerUrl::Staging => "staging",
            IndexerUrl::Devnet => "devnet",
            IndexerUrl::Mainnet => "mainnet",
            IndexerUrl::Custom(url) => {
                return url.clone();
            }
        };
        format!("https://{name}.data-anchor.termina.technology")
    }
}

impl std::str::FromStr for IndexerUrl {
    type Err = IndexerUrlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(IndexerUrlError::InvalidUrl(
                "Indexer URL cannot be empty".to_string(),
            ));
        }
        match s {
            "staging" => Ok(IndexerUrl::Staging),
            "devnet" => Ok(IndexerUrl::Devnet),
            "mainnet" => Ok(IndexerUrl::Mainnet),
            s if s.starts_with("https://") || s.starts_with("http://") => {
                Ok(IndexerUrl::Custom(s.to_string()))
            }
            _ => Err(IndexerUrlError::InvalidUrl(format!(
                "URL not a valid protocol, expected `http(s)`: {s}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::mainnet(MAINNET_GENESIS_HASH, Ok(IndexerUrl::Mainnet))]
    #[case::devnet(DEVNET_GENESIS_HASH, Ok(IndexerUrl::Devnet))]
    #[case::testnet(TESTNET_GENESIS_HASH, Err(IndexerUrlError::TestnetNotSupported))]
    #[case::unknown("unknown_genesis_hash", Err(IndexerUrlError::UnknownGenesisHash("unknown_genesis_hash".to_string())))]
    fn test_indexer_url_from_genesis_hash(
        #[case] genesis_hash: &str,
        #[case] expected: IndexerUrlResult<IndexerUrl>,
    ) {
        assert_eq!(IndexerUrl::from_genesis_hash(genesis_hash), expected);
    }

    #[rstest]
    #[case::mainnet(IndexerUrl::Mainnet, "https://mainnet.data-anchor.termina.technology")]
    #[case::devnet(IndexerUrl::Devnet, "https://devnet.data-anchor.termina.technology")]
    #[case::staging(IndexerUrl::Staging, "https://staging.data-anchor.termina.technology")]
    #[case::custom(IndexerUrl::Custom("https://custom.indexer.url".to_string()), "https://custom.indexer.url")]
    fn test_indexer_url_url(#[case] indexer_url: IndexerUrl, #[case] expected_url: &str) {
        assert_eq!(indexer_url.url(), expected_url);
    }

    #[rstest]
    #[case::valid("https://custom.indexer.url", Ok(IndexerUrl::Custom("https://custom.indexer.url".to_string())))]
    #[case::staging("staging", Ok(IndexerUrl::Staging))]
    #[case::devnet("devnet", Ok(IndexerUrl::Devnet))]
    #[case::mainnet("mainnet", Ok(IndexerUrl::Mainnet))]
    #[case::invalid("invalid_url", Err(IndexerUrlError::InvalidUrl("URL not a valid protocol, expected `http(s)`: invalid_url".to_string())))]
    fn test_indexer_url_from_str(
        #[case] input: &str,
        #[case] expected: IndexerUrlResult<IndexerUrl>,
    ) {
        assert_eq!(IndexerUrl::from_str(input), expected);
    }
}
