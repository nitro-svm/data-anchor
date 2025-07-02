use std::{collections::HashSet, str::FromStr};

use chrono::{DateTime, Utc};
use jsonrpsee::{
    core::{RpcResult, SubscriptionResult},
    proc_macros::rpc,
};
use serde::{Deserialize, Deserializer, Serialize, de};
use solana_sdk::{clock::Slot, pubkey::Pubkey};

use crate::CompoundProof;

/// A data structure representing a blober's information, including the blober's pubkey, the
/// payer's pubkey, and the network of the blober.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BloberData {
    #[serde(deserialize_with = "deserialize_pubkey")]
    pub blober: Pubkey,
    pub payer: Pubkey,
    pub network_id: u64,
}

/// A time range with optional start and end times, used for filtering time.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeRange {
    /// The start time of the range, inclusive.
    pub start: Option<DateTime<Utc>>,
    /// The end time of the range, inclusive.
    pub end: Option<DateTime<Utc>>,
}

impl TimeRange {
    /// Returns the start and end times as a tuple of `DateTime<Utc>`, with defaults for
    /// missing values.
    pub fn to_db_defaults(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        #[allow(clippy::unwrap_used, reason = "Hardcoding 0 will never panic")]
        let default_start = DateTime::<Utc>::from_timestamp_micros(0).unwrap();

        (
            self.start.unwrap_or(default_start),
            self.end.unwrap_or(Utc::now()),
        )
    }
}

/// A wrapper around a blober's pubkey, used to identify a blober in RPC calls.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PubkeyFromStr(#[serde(deserialize_with = "deserialize_pubkey")] pub Pubkey);

impl From<PubkeyFromStr> for Pubkey {
    fn from(value: PubkeyFromStr) -> Self {
        value.0
    }
}

impl From<Pubkey> for PubkeyFromStr {
    fn from(value: Pubkey) -> Self {
        PubkeyFromStr(value)
    }
}

/// The Indexer RPC interface.
#[rpc(server, client)]
pub trait IndexerRpc {
    /// Retrieve a list of blobs for a given slot and blober pubkey. Returns an error if there was a
    /// database or RPC failure, and None if the slot has not been completed yet. If the slot is
    /// completed, an empty list will be returned.
    #[method(name = "get_blobs")]
    async fn get_blobs(&self, blober: PubkeyFromStr, slot: u64) -> RpcResult<Option<Vec<Vec<u8>>>>;

    /// Retrieve a list of blobs for a given blober pubkey and time range. Returns an error if there
    /// was a database or RPC failure, and an empty list if no blobs were found.
    #[method(name = "get_blobs_by_blober")]
    async fn get_blobs_by_blober(
        &self,
        blober: PubkeyFromStr,
        time_range: Option<TimeRange>,
    ) -> RpcResult<Vec<Vec<u8>>>;

    /// Retrieve a list of blobs for a given payer pubkey, network ID, and time range. Returns an
    /// error if there was a database or RPC failure, and an empty list if no blobs were found.
    #[method(name = "get_blobs_by_payer")]
    async fn get_blobs_by_payer(
        &self,
        payer: PubkeyFromStr,
        network_name: String,
        time_range: Option<TimeRange>,
    ) -> RpcResult<Vec<Vec<u8>>>;

    /// Retrieve a list of blobs for a given network name and time range. Returns an error if there
    /// was a database or RPC failure, and an empty list if no blobs were found.
    #[method(name = "get_blobs_by_network")]
    async fn get_blobs_by_network(
        &self,
        network_name: String,
        time_range: TimeRange,
    ) -> RpcResult<Vec<Vec<u8>>>;

    /// Retrieve a list of blobs for a given namespace and time range. Returns an error if there
    /// was a database or RPC failure, and an empty list if no blobs were found.
    #[method(name = "get_blobs_by_namespace")]
    async fn get_blobs_by_namespace_for_payer(
        &self,
        namespace: String,
        payer: Option<PubkeyFromStr>,
        time_range: TimeRange,
    ) -> RpcResult<Vec<Vec<u8>>>;

    /// Retrieve a list of payers for a given network name. Returns an error if there was a
    /// database or RPC failure, and an empty list if no payers were found.
    #[method(name = "get_payers_by_network")]
    async fn get_payers_by_network(&self, network_name: String) -> RpcResult<Vec<PubkeyFromStr>>;

    /// Retrieve a proof for a given slot and blober pubkey. Returns an error if there was a
    /// database or RPC failure, and None if the slot has not been completed yet.
    #[method(name = "get_proof")]
    async fn get_proof(&self, blober: PubkeyFromStr, slot: u64)
    -> RpcResult<Option<CompoundProof>>;

    /// Retrieve a compound proof that covers a particular blob. Returns an error if there was a
    /// database or RPC failure, and None if the blob does not exist.
    #[method(name = "get_proof_for_blob")]
    async fn get_proof_for_blob(
        &self,
        blob_address: PubkeyFromStr,
    ) -> RpcResult<Option<CompoundProof>>;

    /// Add a list of blober PDA addresses to the list of tracked blobers.
    #[method(name = "add_blobers")]
    async fn add_blobers(&self, blobers: HashSet<BloberData>) -> RpcResult<()>;

    /// Remove a list of blober PDA addresses from the list of tracked blobers.
    #[method(name = "remove_blobers")]
    async fn remove_blobers(&self, blobers: HashSet<PubkeyFromStr>) -> RpcResult<()>;

    /// Listen to blob finalization events from specified blobers. This will return a stream of
    /// slots and blober PDAs that have finalized blobs. The stream will be closed when the RPC server is
    /// shut down.
    #[subscription(name = "subscribe_blob_finalization" => "listen_subscribe_blob_finalization", unsubscribe = "unsubscribe_blob_finalization", item = (Pubkey, Slot))]
    async fn subscribe_blob_finalization(
        &self,
        blobers: HashSet<PubkeyFromStr>,
    ) -> SubscriptionResult;
}

fn deserialize_pubkey<'de, D>(deserializer: D) -> Result<Pubkey, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)
        .and_then(|key| Pubkey::from_str(&key).map_err(de::Error::custom))
}
