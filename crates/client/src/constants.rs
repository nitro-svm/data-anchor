/// Default number of concurrent requests to send to the RPC.
pub const DEFAULT_CONCURRENCY: usize = 100;

/// Default number of slots to look back for the
/// [`crate::blober_client::BloberClient::get_ledger_blobs`] method.
pub const DEFAULT_LOOKBACK_SLOTS: u64 = 10;
