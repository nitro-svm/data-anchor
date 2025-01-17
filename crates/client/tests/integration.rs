use std::{net::Ipv4Addr, sync::Arc, time::Duration};

use anyhow::Context;
use gadgets_scfs::ScfsMatrix;
use nitro_da_client::*;
use solana_client::{
    nonblocking::{rpc_client::RpcClient, tpu_client::TpuClient},
    tpu_client::TpuClientConfig,
};
use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair, signer::Signer};
use solana_test_validator::TestValidatorGenesis;
use tracing::{info, Level};
use tracing_subscriber::{filter::FilterFn, prelude::*, util::SubscriberInitExt, EnvFilter};

#[tokio::test]
#[ignore = "Test validator is currently failing in release mode with SIGILL. Debug still works."]
async fn full_workflow() {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_line_number(true)
        .pretty();
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(FilterFn::new(|s| {
            !s.target().starts_with("solana_") || *s.level() > Level::INFO
        }))
        .init();

    if let Err(e) = assert_program_is_up_to_date("chunker") {
        eprintln!("WARNING: Failed to determine if the chunker program is up to date. This should not happen in normal tests.");
        eprintln!("{e}");
        eprintln!("{e:#?}");
    }
    if let Err(e) = assert_program_is_up_to_date("hasher") {
        eprintln!("WARNING: Failed to determine if the hasher program is up to date. This should not happen in normal tests.");
        eprintln!("{e}");
        eprintln!("{e:#?}");
    }

    // This will work out the set of features that are inactive in any cluster. (Local, dev, test or mainnet)
    // The purpose is to ensure the transaction compiles and is usable on all clusters.
    let inactive_features = {
        let mut matrix = ScfsMatrix::new(None).unwrap();
        matrix.run().await.unwrap();

        matrix
            .get_features(Some(&ScfsMatrix::any_inactive))
            .unwrap()
    };

    info!("Creating test validator");
    let base_dir = format!(
        "{}/../da_programs/target/deploy/",
        env!("CARGO_MANIFEST_DIR")
    );
    let validator_genesis = {
        let mut validator = TestValidatorGenesis::default();
        validator
            .add_program(&(base_dir.clone() + "chunker"), chunker::id())
            .add_program(&(base_dir + "hasher"), hasher::id())
            .bind_ip_addr(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST))
            .rpc_port(8899)
            .deactivate_features(&inactive_features);
        validator
    };

    info!("Starting test validator");
    let (test_validator, payer) = validator_genesis.start_async().await;

    // Convert from remote agave Keypair to local nitro Keypair
    let payer = Keypair::from_bytes(&payer.to_bytes()).unwrap();
    let payer = Arc::new(payer);

    // Convert from remote agave RpcClient to local nitro RpcClient
    let rpc_client =
        RpcClient::new_with_commitment(test_validator.rpc_url(), CommitmentConfig::processed());
    let rpc_client = Arc::new(rpc_client);
    // Sending too many transactions at once can cause the test validator to hang. It seems to hit
    // some deadlock with the JsonRPC server shutdown. This is a test, so leak it to keep tests moving.
    std::mem::forget(test_validator);

    let data_len = 200 * 1024;
    let data: Vec<u8> = (0..data_len).map(|i: u32| (i % 255) as u8).collect();

    let hasher_client = HasherClient::new(payer.clone(), rpc_client.clone());
    let hasher = hasher_client
        .create_hasher(None, FeeStrategy::Fixed(Fee::ZERO))
        .await
        .unwrap();

    let balance_before = rpc_client.get_balance(&payer.pubkey()).await.unwrap();

    let tpu_client = Arc::new(
        TpuClient::new("test", rpc_client.clone(), "", TpuClientConfig::default())
            .await
            .unwrap(),
    );
    let batch_client = BatchClient::new(rpc_client.clone(), Some(tpu_client), vec![payer.clone()])
        .await
        .unwrap();

    let chunker_client = ChunkerClient::new(payer.clone(), rpc_client.clone(), batch_client);

    let priority = Priority::default();
    let mut expected_fee = chunker_client
        .estimate_fees(data.len(), priority)
        .await
        .unwrap();

    expected_fee.prioritization_fee_rate = MicroLamports::new(500);

    info!("Uploading blob");
    chunker_client
        .upload_blob(
            &data,
            FeeStrategy::Fixed(expected_fee),
            hasher.pubkey(),
            Some(Duration::from_secs(20)),
        )
        .await
        .unwrap();
    info!("Done");

    let balance_after = rpc_client.get_balance(&payer.pubkey()).await.unwrap();

    let actual = (balance_before - balance_after) as f32;
    let expected = expected_fee.total_fee().into_inner() as f32;
    let percent_diff = ((actual / expected) - 1.0).abs();
    info!(
        "Balance before: {} lamports, balance after: {} lamports, expected fee was: {}, actual fee was: {}",
        balance_before,
        balance_after,
        expected_fee.total_fee(),
        (balance_before - balance_after)
    );
    info!("percent difference: {:.2}%", percent_diff * 100.0);
    assert!(percent_diff < 0.01);
}

/// Verify that the contract program has been built more recently than its code was modified.
fn assert_program_is_up_to_date(program: &str) -> anyhow::Result<()> {
    let program_path = format!(
        "{}/../da_programs/target/deploy/{}.so",
        env!("CARGO_MANIFEST_DIR"),
        program
    );
    let source_dir = format!(
        "{}/../da_programs/programs/{}",
        env!("CARGO_MANIFEST_DIR"),
        program
    );

    let program_mtime = std::fs::metadata(program_path.clone())
        .context(program_path)?
        .modified()?;
    let sources = walkdir::WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file());

    for source in sources {
        let source_mtime = source
            .path()
            .metadata()
            .context(source.path().to_string_lossy().to_string())?
            .modified()?;
        assert!(source_mtime < program_mtime, "The program {} is not up to date. Please run `anchor build` in the da_programs directory.", program);
    }
    Ok(())
}
