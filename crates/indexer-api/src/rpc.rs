use std::collections::HashSet;

use anchor_lang::prelude::Pubkey;
use chrono::{DateTime, Utc};
use clap::ValueEnum;
use data_anchor_blober::GROTH16_PROOF_SIZE;
use data_anchor_proofs::compound::CompoundInclusionProof;
use jsonrpsee::{
    core::{RpcResult, SubscriptionResult},
    proc_macros::rpc,
};
use serde::{Deserialize, Serialize};

/// A data structure representing a blober's information, including the blober's pubkey, the
/// payer's pubkey, and the network of the blober.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BloberData {
    #[serde(with = "pubkey_with_str")]
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
pub struct PubkeyFromStr(#[serde(with = "pubkey_with_str")] pub Pubkey);

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

/// Data structure to hold the proof data
#[serde_with::serde_as]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofData {
    /// The Groth16 proof bytes
    #[serde_as(as = "serde_with::Bytes")]
    pub proof: [u8; GROTH16_PROOF_SIZE],
    /// The public values from the proof
    pub public_values: Vec<u8>,
    /// The verification key bytes in hex encoding with a leading "0x"
    pub verification_key: String,
}

/// The Indexer RPC interface.
#[rpc(server, client)]
pub trait IndexerRpc {
    /// Check the health of the RPC server. Returns an error if the server is not healthy.
    #[method(name = "health")]
    async fn health(&self) -> RpcResult<()>;

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
        time_range: Option<TimeRange>,
    ) -> RpcResult<Vec<Vec<u8>>>;

    /// Retrieve a list of blobs for a given namespace and time range. Returns an error if there
    /// was a database or RPC failure, and an empty list if no blobs were found.
    #[method(name = "get_blobs_by_namespace")]
    async fn get_blobs_by_namespace_for_payer(
        &self,
        namespace: String,
        payer: Option<PubkeyFromStr>,
        time_range: Option<TimeRange>,
    ) -> RpcResult<Vec<Vec<u8>>>;

    /// Retrieve a list of payers for a given network name. Returns an error if there was a
    /// database or RPC failure, and an empty list if no payers were found.
    #[method(name = "get_payers_by_network")]
    async fn get_payers_by_network(&self, network_name: String) -> RpcResult<Vec<PubkeyFromStr>>;

    /// Retrieve a proof for a given slot and blober pubkey. Returns an error if there was a
    /// database or RPC failure, and None if the slot has not been completed yet.
    #[deprecated(since = "0.4.3", note = "please use `checkpoint_proof` instead")]
    #[method(name = "get_proof")]
    async fn get_proof(
        &self,
        blober: PubkeyFromStr,
        slot: u64,
    ) -> RpcResult<Option<CompoundInclusionProof>>;

    /// Retrieve a compound proof that covers a particular blob. Returns an error if there was a
    /// database or RPC failure, and None if the blob does not exist.
    #[deprecated(since = "0.4.3", note = "please use `checkpoint_proof` instead")]
    #[method(name = "get_proof_for_blob")]
    async fn get_proof_for_blob(
        &self,
        blob_address: PubkeyFromStr,
    ) -> RpcResult<Option<CompoundInclusionProof>>;

    /// Listen to blob finalization events from specified blobers. This will return a stream of
    /// slots and blober PDAs that have finalized blobs. The stream will be closed when the RPC server is
    /// shut down.
    #[subscription(
        name = "subscribe_blob_finalization" => "listen_subscribe_blob_finalization",
        unsubscribe = "unsubscribe_blob_finalization", 
        item = (Pubkey, u64)
    )]
    async fn subscribe_blob_finalization(
        &self,
        blobers: HashSet<PubkeyFromStr>,
    ) -> SubscriptionResult;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum CustomerElf {
    /// Data correctness elf, commits to the data being correct.
    DataCorrectness,
    /// Dawn SLA elf, which commits to the data being correct and to a SLA result.
    DawnSla,
}

impl std::fmt::Display for CustomerElf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CustomerElf::DataCorrectness => write!(f, "data-correctness"),
            CustomerElf::DawnSla => write!(f, "dawn-sla"),
        }
    }
}

impl CustomerElf {
    pub fn authority(&self) -> Pubkey {
        match self {
            CustomerElf::DataCorrectness => data_anchor_data_correctness_verifier::id(),
            CustomerElf::DawnSla => data_anchor_dawn_sla_verifier::id(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i8)]
pub enum RequestFailureReason {
    #[default]
    Unknown,
    ProofGenerationFailed,
    TransactionError,
    RpcConnection,
}

impl From<RequestFailureReason> for i16 {
    fn from(reason: RequestFailureReason) -> Self {
        match reason {
            RequestFailureReason::Unknown => -1,
            RequestFailureReason::ProofGenerationFailed => -2,
            RequestFailureReason::TransactionError => -3,
            RequestFailureReason::RpcConnection => -4,
        }
    }
}

impl From<i16> for RequestFailureReason {
    fn from(reason: i16) -> Self {
        match reason {
            -1 => RequestFailureReason::Unknown,
            -2 => RequestFailureReason::ProofGenerationFailed,
            -3 => RequestFailureReason::TransactionError,
            -4 => RequestFailureReason::RpcConnection,
            #[allow(
                clippy::panic,
                reason = "This should never happen as we only use this for reading from the database"
            )]
            _ => panic!("Invalid request failure reason: {reason}"),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i8)]
pub enum RequestStatus {
    #[default]
    Created,
    Submitted,
    Completed,
    Posted,
    Failed(RequestFailureReason),
}

impl From<RequestStatus> for i16 {
    fn from(status: RequestStatus) -> Self {
        match status {
            RequestStatus::Created => 0,
            RequestStatus::Submitted => 1,
            RequestStatus::Completed => 2,
            RequestStatus::Posted => 3,
            RequestStatus::Failed(reason) => reason.into(),
        }
    }
}

impl From<i16> for RequestStatus {
    fn from(status: i16) -> Self {
        match status {
            0 => RequestStatus::Created,
            1 => RequestStatus::Submitted,
            2 => RequestStatus::Completed,
            3 => RequestStatus::Posted,
            x if x < 0 => RequestStatus::Failed(x.into()),
            #[allow(
                clippy::panic,
                reason = "This should never happen as we only use this for reading from the database"
            )]
            _ => panic!("Invalid request status: {status}"),
        }
    }
}

/// The Proof RPC interface.
#[rpc(server, client)]
pub trait ProofRpc {
    /// Check the health of the RPC server. Returns an error if the server is not healthy.
    #[method(name = "health")]
    async fn health(&self) -> RpcResult<()>;

    /// Request building a succinct ZK Groth16 proof for a given blober and slot. (Custom per
    /// client)
    #[method(name = "checkpoint_proof")]
    async fn checkpoint_proof(
        &self,
        blober: PubkeyFromStr,
        slot: u64,
        customer_elf: CustomerElf,
    ) -> RpcResult<String>;

    /// Get a proof request status by its ID. Returns an error if the request does not exist or
    /// if there was a database or RPC failure.
    #[method(name = "get_proof_request_status")]
    async fn get_proof_request_status(&self, request_id: String) -> RpcResult<RequestStatus>;
}

pub mod pubkey_with_str {
    use std::str::FromStr;

    use anchor_lang::prelude::Pubkey;
    use serde::{Deserialize, Deserializer, de};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Pubkey, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)
            .and_then(|key| Pubkey::from_str(&key).map_err(de::Error::custom))
    }

    pub fn serialize<S>(pubkey: &Pubkey, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&pubkey.to_string())
    }
}
