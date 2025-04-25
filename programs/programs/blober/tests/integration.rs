use std::time::SystemTime;

use anchor_lang::{
    solana_program::instruction::Instruction, AccountDeserialize, Discriminator, InstructionData,
    Space, ToAccountMetas,
};
use futures::{stream::FuturesOrdered, StreamExt};
use nitro_da_blober::{
    accounts, find_blob_address, find_blober_address, hash_leaf, instruction, state::blob::Blob,
    CHUNK_SIZE,
};
use rand::prelude::SliceRandom;
use solana_program_test::*;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig, hash, signature::Signer, transaction::Transaction,
};
use solana_test_validator::TestValidatorGenesis;

#[tokio::test]
async fn test_validator_transaction() {
    solana_logger::setup_with_default("solana_program_runtime=debug");
    let program_id = nitro_da_blober::id();
    let system_program = anchor_lang::solana_program::system_program::id();

    let (test_validator, payer) = TestValidatorGenesis::default()
        .add_program(
            "../../target/deploy/nitro_da_blober",
            program_id.to_bytes().into(),
        )
        .start_async()
        .await;
    let rpc_client =
        RpcClient::new_with_commitment(test_validator.rpc_url(), CommitmentConfig::processed());
    let rpc_client = &rpc_client;
    // Sending too many transactions at once can cause the test validator to hang. It seems to hit
    // some deadlock with the JsonRPC server shutdown. This is a test, so leak it to keep tests moving.
    std::mem::forget(test_validator);

    let blober = find_blober_address(program_id, payer.pubkey(), "test");

    // Create blober account.
    {
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::Initialize {
                    blober,
                    payer: payer.pubkey(),
                    system_program,
                }
                .to_account_metas(None),
                data: instruction::Initialize {
                    namespace: "test".to_string(),
                    trusted: payer.pubkey(),
                }
                .data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            rpc_client.get_latest_blockhash().await.unwrap(),
        );

        let sig = rpc_client
            .send_transaction(&transaction)
            .await
            .expect("failed to initialize blober");
        rpc_client.poll_for_signature(&sig).await.unwrap();
    }

    let data_len = 20 * 1024;
    let data: Vec<u8> = (0u8..255).cycle().take(data_len).collect();

    let mut chunks = data
        .chunks(CHUNK_SIZE as usize)
        .enumerate()
        .collect::<Vec<_>>();
    println!(
        "account size: {} bytes",
        Blob::DISCRIMINATOR.len() + Blob::INIT_SPACE
    );

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let blob = find_blob_address(program_id, payer.pubkey(), blober, timestamp, data.len());

    // Create blob
    {
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::DeclareBlob {
                    blob,
                    blober,
                    payer: payer.pubkey(),
                    system_program,
                }
                .to_account_metas(None),
                data: instruction::DeclareBlob {
                    timestamp,
                    blob_size: data.len() as u32,
                }
                .data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            rpc_client.get_latest_blockhash().await.unwrap(),
        );

        let sig = rpc_client
            .send_transaction(&transaction)
            .await
            .expect("failed to create blob");
        rpc_client.poll_for_signature(&sig).await.unwrap();
    }

    // Randomly permute the chunks to make sure out-of-order submissions work.
    chunks.shuffle(&mut rand::thread_rng());

    // This runs all chunks in sequence, and makes sure each transaction is in a new block.
    // It's the easiest way to ensure we can build the blob digest off-chain at the same time for the test.
    let recent_blockhash = rpc_client.get_latest_blockhash().await.unwrap();
    let mut signatures = FuturesOrdered::new();
    let mut incremental_hash = hash::Hasher::default().result().to_bytes();
    for (chunk_index, chunk_data) in chunks {
        incremental_hash = hash_leaf(incremental_hash, chunk_index as u16, chunk_data);
        println!("trying to insert chunk {chunk_index}");
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::InsertChunk {
                    blob,
                    blober,
                    payer: payer.pubkey(),
                }
                .to_account_metas(None),
                data: instruction::InsertChunk {
                    idx: chunk_index as u16,
                    data: chunk_data.to_vec(),
                }
                .data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );
        signatures.push_back(async move {
            (
                chunk_index,
                rpc_client
                    .send_transaction(&transaction)
                    .await
                    .unwrap_or_else(|e| panic!("failed to upload chunk {}, {e:?}", chunk_index)),
            )
        });

        let (_, sig) = signatures.next().await.unwrap();
        rpc_client.poll_for_signature(&sig).await.unwrap();
    }

    // Get digest from blob
    let blob = rpc_client.get_account(&blob).await.unwrap();

    let blob = Blob::try_deserialize(&mut &blob.data[..]).expect("failed to deserialize blob");

    dbg!(&blob);
    assert_eq!(blob.blob_digest(), &incremental_hash);
}
