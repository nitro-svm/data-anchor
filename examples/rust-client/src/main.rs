use std::{path::PathBuf, sync::Arc, time::Duration};

use clap::Parser;
use data_anchor_client::{DataAnchorClient, FeeStrategy};
use serde_json::json;
use solana_cli_config::Config;
use solana_keypair::Keypair;
use solana_signer::{EncodableKey, Signer};
use tokio_util::sync::CancellationToken;

#[derive(Debug, clap::Parser)]
struct Args {
    /// The namespace to use for the blober (must be unique on-chain).
    #[arg(long, env = "DATA_ANCHOR_NAMESPACE")]
    namespace: String,

    /// The path to the keypair file for the payer account.
    #[arg(long, env = "PAYER_KEYPAIR_PATH")]
    payer_keypair_path: PathBuf,

    /// Optional indexer API token for authenticated access.
    #[arg(long, env = "DATA_ANCHOR_INDEXER_API_TOKEN")]
    indexer_api_token: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting client example...");

    // ─── Load env ─────────────────────────────────────────────────────────────────
    println!("Reading environment variables...");
    dotenvy::dotenv()?;
    let args = Args::parse();

    println!("Environment variables loaded:");
    println!("Namespace: {}", args.namespace);
    println!(
        "Keypair path: {}",
        args.payer_keypair_path.to_string_lossy()
    );

    // Load the Solana keypair that will pay for transactions and own the data
    println!("Loading keypair...");
    let payer = Arc::new(Keypair::read_from_file(args.payer_keypair_path)?);
    println!("Keypair loaded: {}", payer.pubkey());

    // Build DataAnchor client
    println!("Building DataAnchor client...");
    let config = Config::load(solana_cli_config::CONFIG_FILE.as_ref().unwrap())?;
    println!("Using RPC URL: {}", config.json_rpc_url);
    let cancellation_token = CancellationToken::new();

    let client = DataAnchorClient::builder()
        .payer(payer.clone())
        .maybe_indexer(None)
        .build_with_config(
            config,
            cancellation_token.clone(),
            args.indexer_api_token.clone(),
        )
        .await?;
    println!("Client built successfully!");

    // ─── 1. Initialize blober ─────────────────────────────────────────────────────
    // Create the on-chain storage container (PDA) for our namespace
    println!("\n1. Initializing Data Anchor blober");
    match client
        .initialize_blober(FeeStrategy::default(), args.namespace.clone().into(), None)
        .await
    {
        Ok(_) => println!("Blober initialized for namespace '{}'", args.namespace),
        Err(e) => {
            if e.to_string().contains("AccountExists")
                || e.to_string().contains("Account already exists")
            {
                println!(
                    "Blober already exists for namespace '{}', continuing...",
                    args.namespace
                );
            } else {
                return Err(e.into());
            }
        }
    }

    // ─── 2. Write dynamic JSON blob (can be skipped if you want to upload a static blob) ───────────────────────────────
    // Build a JSON file with current rewards metrics
    println!("\n2. Create rewards payload");
    let payload_json = json!({
        "epoch": 1042,
        "timestamp": "2025-07-25T17:09:00+08:00",
        "location": "Zug, Switzerland",
        "devices": [
            { "device_id": "sensor-001", "data_points": 340, "co2_ppm": 417, "reward": "0.03" },
            { "device_id": "sensor-002", "data_points": 327, "co2_ppm": 419, "reward": "0.02" }
        ],
        "total_reward": "0.05",
        "proof": "mock_zk_proof_here"
    });
    let payload = serde_json::to_string(&payload_json)?.into_bytes();
    println!(
        "  payload: {}",
        serde_json::to_string_pretty(&payload_json)?
    );

    // ─── 3. Upload blob ───────────────────────────────────────────────────────────
    // Send the payload on-chain, capture its signature and ledger slot
    println!("\n3. Uploading blob");
    let (outcomes, _blob_addr) = client
        .upload_blob(
            &payload,
            FeeStrategy::default(),
            &args.namespace,
            Some(Duration::from_secs(10)),
        )
        .await?;
    if outcomes.is_empty() {
        panic!("No upload outcomes returned");
    }
    let slot = outcomes[0].slot;
    let sigs: Vec<_> = outcomes.iter().map(|o| o.signature).collect();
    println!("  signature: {:?}", sigs[0]);
    println!("  slot:      {}", slot);

    // ─── 4. Fetch from ledger by signature ────────────────────────────────────────
    // Retrieve via ledger and decode back to JSON
    println!("\n4. Fetching from ledger (by signature)");
    let recovered: Vec<u8> = client
        .get_ledger_blobs_from_signatures(args.namespace.clone().into(), sigs.clone())
        .await?;
    assert_eq!(recovered, payload);

    // Verify the retrieved data matches what we uploaded
    let recovered_json: serde_json::Value = serde_json::from_slice(&recovered)?;
    println!("  fetched data matches original:");
    println!("{}", serde_json::to_string_pretty(&recovered_json)?);

    if matches!(args.indexer_api_token, Some(t) if !t.is_empty()) {
        // ─── 5. Query via indexer by slot ─────────────────────────────────────────────
        // Query the indexer for metadata & raw data at that slot
        println!("\n5. Fetching blob via indexer");
        let blobs = client
            .get_blobs::<Vec<u8>>(slot, args.namespace.clone().into())
            .await?;
        if let Some(blobs) = blobs {
            println!("Indexer returned {} blob(s)", blobs.len());
        } else {
            println!("Indexer returned no blobs");
        }
    } else {
        println!("\nSkipping step 5 because no DATA_ANCHOR_INDEXER_API_TOKEN is set");
    }

    // ─── 6. Close blober ─────────────────────────────────────────────────────────
    // Tear down the on-chain storage account to reclaim rent
    println!("\n6. Closing Data Anchor blober");
    client
        .close_blober(FeeStrategy::default(), args.namespace.clone().into(), None)
        .await?;
    println!("Blober closed");

    println!(
        "\nYou've successfully initialized a namespace, uploaded & verified a blob, queried the indexer, fetched a proof, and closed the namespace."
    );
    Ok(())
}
