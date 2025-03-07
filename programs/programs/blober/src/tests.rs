use anchor_lang::{
    prelude::{AccountInfo, Pubkey},
    solana_program::{self, instruction::Instruction},
    AccountDeserialize, InstructionData, ToAccountMetas,
};
use rand::{prelude::SliceRandom, thread_rng};
use solana_program_test::*;
use solana_sdk::{
    clock::Clock,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

use crate::{
    accounts, compute_blob_digest, find_blob_address, find_blober_address, hash_blob, id,
    instruction, merge_hashes,
    state::{blob::Blob, blober::Blober},
    try_entry, CHUNK_SIZE,
};

#[test]
fn blob_digest() {
    // This is a snapshot test to ensure the blob digest doesn't change unexpectedly.
    let data_len = 100 * 1024;
    let data: Vec<_> = (0u8..255).cycle().take(data_len).collect();

    let expected_blob_digest = "6a30e7413c9893dadd2bdad25da4b2012ca63a1fd48736f9323a5bb1981a2a24";

    let chunks = data
        .chunks(CHUNK_SIZE as usize)
        .enumerate()
        .map(|(i, chunk)| (i as u16, chunk))
        .collect::<Vec<_>>();
    assert_eq!(
        hex::encode(compute_blob_digest(&chunks)),
        expected_blob_digest
    );
}

// This is a copy of the macro-generated `entry` function but adjusted
// to fit with what solana_program_test::processor! expects.
// See also: https://github.com/coral-xyz/anchor/pull/2711
//           https://github.com/coral-xyz/anchor/issues/2738
//           https://github.com/dankelleher/solana/commit/3c285b5574722bd8e7ec4c7f659ec769b9aba5ce
fn test_entry(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    data: &[u8],
) -> anchor_lang::solana_program::entrypoint::ProgramResult {
    // Leak is okay because it's just for testing.
    let accounts = accounts.to_vec().leak();
    try_entry(program_id, accounts, data).map_err(move |e| {
        e.log();
        e.into()
    })
}

async fn process_transaction(
    banks_client: &mut BanksClient,
    transaction: Transaction,
) -> std::result::Result<(), BanksClientError> {
    let tx = banks_client
        .process_transaction_with_metadata(transaction)
        .await?;

    println!("tx {:?}", tx.metadata);
    tx.result?;

    Ok(())
}

async fn upload_blob(
    program_id: Pubkey,
    payer: Keypair,
    system_program: Pubkey,
    data: &[u8],
    banks_client: &mut BanksClient,
    timestamp: u64,
    blober: Pubkey,
) -> (Pubkey, [u8; 32]) {
    let chunks = data
        .chunks(CHUNK_SIZE as usize)
        .enumerate()
        .map(|(i, chunk)| (i as u16, chunk))
        .collect::<Vec<_>>();

    let blob_digest = compute_blob_digest(&chunks);

    println!("num chunks: {} * {}", chunks.len(), chunks[0].1.len());

    let blob = find_blob_address(payer.pubkey(), blober, timestamp);
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
                    num_chunks: chunks.len() as u16,
                }
                .data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(banks_client, transaction)
            .await
            .expect("failed to create blob");
    }

    // This runs all chunks in sequence
    for (idx, chunk_data) in &chunks {
        println!("chunk {idx}");
        let mut banks_client = banks_client.clone();
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
                    idx: *idx,
                    data: chunk_data.to_vec(),
                }
                .data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            banks_client.get_latest_blockhash().await.unwrap(),
        );
        process_transaction(&mut banks_client, transaction)
            .await
            .unwrap_or_else(|_| panic!("failed to upload chunk {idx}"));
    }

    (blob, blob_digest)
}

#[tokio::test]
async fn test_100k_blob() {
    solana_logger::setup();
    let program_id = solana_sdk::pubkey::Pubkey::new_from_array(id().to_bytes());
    let system_program = solana_program::system_program::id();

    println!("program_id: {:?}", program_id);
    println!("system_program: {:?}", system_program);
    let program_test = ProgramTest::new("blob", program_id, processor!(test_entry));
    let (mut banks_client, payer, _) = program_test.start().await;

    let blober = find_blober_address(payer.pubkey(), "test");

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
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to create blober account");
    }

    let data_len = 100 * 1024;
    let data: Vec<_> = (0u8..255).cycle().take(data_len).collect();

    let (blob, blob_digest) = upload_blob(
        program_id,
        payer,
        system_program,
        &data,
        &mut banks_client,
        0,
        blober,
    )
    .await;

    let blob = banks_client.get_account(blob).await.unwrap().unwrap();

    let blob = Blob::try_deserialize(&mut &blob.data[..]).expect("failed to deserialize blob");

    println!("blob: {blob:?}");
    assert_eq!(blob.blob_digest(), &blob_digest);
}

#[tokio::test]
async fn idle_blob_fails() {
    let program_id = id();
    let system_program = solana_program::system_program::id();

    let program_test = ProgramTest::new("blob", program_id, processor!(test_entry));
    let mut context = program_test.start_with_context().await;
    let mut banks_client = context.banks_client.clone();
    let payer = context.payer.insecure_clone();

    let data_len = 100 * 1024;
    let data: Vec<_> = (0u8..255).cycle().take(data_len).collect();

    let chunks = data.chunks(CHUNK_SIZE as usize).collect::<Vec<_>>();
    println!("num chunks: {} * {}", chunks.len(), chunks[0].len());

    let blober = find_blober_address(payer.pubkey(), "test");
    let blob = find_blob_address(payer.pubkey(), blober, 0);

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
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to create blober account");
    }

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
                    timestamp: 0,
                    blob_size: data.len() as u32,
                    num_chunks: chunks.len() as u16,
                }
                .data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to create blob");
    }

    // Randomly permute the chunks to simulate them arriving out of order.
    let mut indexed_chunks: Vec<_> = chunks.iter().enumerate().collect();
    indexed_chunks.shuffle(&mut thread_rng());

    // This runs all chunks in sequence
    for (chunk_index, chunk_data) in &indexed_chunks {
        println!("chunk {chunk_index}");
        let mut banks_client = banks_client.clone();
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
                    idx: *chunk_index as u16,
                    data: chunk_data.to_vec(),
                }
                .data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            banks_client.get_latest_blockhash().await.unwrap(),
        );
        // Delay an arbitrary chunk.
        if *chunk_index == 10 {
            // Warp the validator to simulate time passing.
            let current_slot = banks_client.get_sysvar::<Clock>().await.unwrap().slot;
            let target_slot = current_slot + 2000 + 1;
            println!("warping from slot {current_slot} to {target_slot}");
            context.warp_to_slot(target_slot).unwrap();

            // Inserting the chunk should fail.
            process_transaction(&mut banks_client, transaction)
                .await
                .unwrap_err();
            return;
        } else {
            process_transaction(&mut banks_client, transaction)
                .await
                .unwrap_or_else(|_| panic!("failed to upload chunk {chunk_index}"));
        }
    }
}

#[tokio::test]
async fn hash_single_account() {
    let program_id = id();
    let system_program = solana_program::system_program::id();

    let program_test = ProgramTest::new("blober", program_id, processor!(test_entry));
    let random_data: Vec<_> = (0u8..255).cycle().take(10 * 1024).collect();
    let (mut banks_client, payer, _) = program_test.start().await;
    let blober = find_blober_address(payer.pubkey(), "test");

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
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to create blober account");
    }

    let (blob, blob_digest) = upload_blob(
        program_id,
        payer.insecure_clone(),
        system_program,
        &random_data,
        &mut banks_client,
        0,
        blober,
    )
    .await;

    // Hash source account.
    {
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::FinalizeBlob {
                    blober,
                    blob,
                    payer: payer.pubkey(),
                }
                .to_account_metas(None),
                data: instruction::FinalizeBlob {}.data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to hash source account");
    }

    let blober = banks_client.get_account(blober).await.unwrap().unwrap();

    let blober =
        Blober::try_deserialize(&mut &blober.data[..]).expect("failed to deserialize blober");

    let expected_hash = hash_blob(
        &blob,
        &[
            blob_digest.as_ref(),
            (random_data.len() as u32).to_le_bytes().as_ref(),
        ]
        .concat(),
    );

    assert_eq!(blober.hash, expected_hash.as_ref());
}

#[tokio::test]
async fn hash_two_accounts() {
    let program_id = id();
    let system_program = solana_program::system_program::id();

    let program_test = ProgramTest::new("blober", program_id, processor!(test_entry));
    let source1_data: Vec<_> = (0u8..255).cycle().take(10 * 1024).collect();
    let source2_data: Vec<_> = (10u8..100).cycle().take(20 * 1024).collect();

    let (mut banks_client, payer, _) = program_test.start().await;

    let blober = find_blober_address(payer.pubkey(), "test");

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
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to create blober account");
    }

    let (blob1, digest1) = upload_blob(
        program_id,
        payer.insecure_clone(),
        system_program,
        &source1_data,
        &mut banks_client,
        0,
        blober,
    )
    .await;
    let (blob2, digest2) = upload_blob(
        program_id,
        payer.insecure_clone(),
        system_program,
        &source2_data,
        &mut banks_client,
        1,
        blober,
    )
    .await;

    // Hash source account 1.
    {
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::FinalizeBlob {
                    blober,
                    blob: blob1,
                    payer: payer.pubkey(),
                }
                .to_account_metas(None),
                data: instruction::FinalizeBlob {}.data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to hash source account");
    }

    // Hash source account 2.
    {
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::FinalizeBlob {
                    blober,
                    blob: blob2,
                    payer: payer.pubkey(),
                }
                .to_account_metas(None),
                data: instruction::FinalizeBlob {}.data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to hash source account");
    }

    let blober = banks_client.get_account(blober).await.unwrap().unwrap();

    let blober =
        Blober::try_deserialize(&mut &blober.data[..]).expect("failed to deserialize blober");

    let expected_digest = {
        let first_hash = hash_blob(
            &blob1,
            &[
                digest1.as_ref(),
                (source1_data.len() as u32).to_le_bytes().as_ref(),
            ]
            .concat(),
        );
        let second_hash = hash_blob(
            &blob2,
            &[
                digest2.as_ref(),
                (source2_data.len() as u32).to_le_bytes().as_ref(),
            ]
            .concat(),
        );
        merge_hashes(&first_hash, &second_hash)
    };

    assert_eq!(blober.hash, expected_digest.as_ref());
}

#[tokio::test]
async fn hash_three_accounts() {
    let program_id = id();
    let system_program = solana_program::system_program::id();

    let program_test = ProgramTest::new("blober", program_id, processor!(test_entry));
    let source1_data: Vec<_> = (0u8..255).cycle().take(10 * 1024).collect();
    let source2_data: Vec<_> = (10u8..100).cycle().take(20 * 1024).collect();
    let source3_data: Vec<_> = (22u8..24).cycle().take(40 * 1024).collect();

    let (mut banks_client, payer, _) = program_test.start().await;

    let blober = find_blober_address(payer.pubkey(), "test");

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
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to create blober account");
    }

    let (blob1, digest1) = upload_blob(
        program_id,
        payer.insecure_clone(),
        system_program,
        &source1_data,
        &mut banks_client,
        0,
        blober,
    )
    .await;
    let (blob2, digest2) = upload_blob(
        program_id,
        payer.insecure_clone(),
        system_program,
        &source2_data,
        &mut banks_client,
        1,
        blober,
    )
    .await;
    let (blob3, digest3) = upload_blob(
        program_id,
        payer.insecure_clone(),
        system_program,
        &source3_data,
        &mut banks_client,
        2,
        blober,
    )
    .await;

    // Hash source account 1.
    {
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::FinalizeBlob {
                    blober,
                    blob: blob1,
                    payer: payer.pubkey(),
                }
                .to_account_metas(None),
                data: instruction::FinalizeBlob {}.data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to hash source account");
    }

    // Hash source account 2.
    {
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::FinalizeBlob {
                    blober,
                    blob: blob2,
                    payer: payer.pubkey(),
                }
                .to_account_metas(None),
                data: instruction::FinalizeBlob {}.data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to hash source account");
    }

    // Hash source account 3.
    {
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::FinalizeBlob {
                    blober,
                    blob: blob3,
                    payer: payer.pubkey(),
                }
                .to_account_metas(None),
                data: instruction::FinalizeBlob {}.data(),
            }],
            Some(&payer.pubkey()),
            &[&payer],
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to hash source account");
    }

    let blober = banks_client.get_account(blober).await.unwrap().unwrap();

    let blober =
        Blober::try_deserialize(&mut &blober.data[..]).expect("failed to deserialize blober");

    let expected_digest = {
        let first_hash = hash_blob(
            &blob1,
            &[
                digest1.as_ref(),
                (source1_data.len() as u32).to_le_bytes().as_ref(),
            ]
            .concat(),
        );
        let second_hash = hash_blob(
            &blob2,
            &[
                digest2.as_ref(),
                (source2_data.len() as u32).to_le_bytes().as_ref(),
            ]
            .concat(),
        );
        let third_hash = hash_blob(
            &blob3,
            &[
                digest3.as_ref(),
                (source3_data.len() as u32).to_le_bytes().as_ref(),
            ]
            .concat(),
        );
        let first_merged = merge_hashes(&first_hash, &second_hash);
        merge_hashes(&first_merged, &third_hash)
    };

    assert_eq!(blober.hash, expected_digest.as_ref());
}

#[tokio::test]
async fn hash_single_account_in_two_slots() {
    let program_id = id();
    let system_program = solana_program::system_program::id();

    let program_test = ProgramTest::new("blober", program_id, processor!(test_entry));
    let source_data: Vec<_> = (0u8..255).cycle().take(10 * 1024).collect();
    let mut context = program_test.start_with_context().await;

    let blober = find_blober_address(context.payer.pubkey(), "test");

    // Create blober account.
    {
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::Initialize {
                    blober,
                    payer: context.payer.pubkey(),
                    system_program,
                }
                .to_account_metas(None),
                data: instruction::Initialize {
                    namespace: "test".to_string(),
                    trusted: context.payer.pubkey(),
                }
                .data(),
            }],
            Some(&context.payer.pubkey()),
            &[&context.payer],
            context.banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut context.banks_client, transaction)
            .await
            .expect("failed to create blober account");
    }

    let (blob, digest) = upload_blob(
        program_id,
        context.payer.insecure_clone(),
        system_program,
        &source_data,
        &mut context.banks_client,
        0,
        blober,
    )
    .await;

    // Hash source account.
    {
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::FinalizeBlob {
                    blober,
                    blob,
                    payer: context.payer.pubkey(),
                }
                .to_account_metas(None),
                data: instruction::FinalizeBlob {}.data(),
            }],
            Some(&context.payer.pubkey()),
            &[&context.payer],
            context.banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut context.banks_client, transaction)
            .await
            .expect("failed to hash source account");
    }

    let blober_1 = context
        .banks_client
        .get_account(blober)
        .await
        .unwrap()
        .unwrap();

    let blober_1 =
        Blober::try_deserialize(&mut &blober_1.data[..]).expect("failed to deserialize blober");

    let expected_digest = hash_blob(
        &blob,
        &[
            digest.as_ref(),
            (source_data.len() as u32).to_le_bytes().as_ref(),
        ]
        .concat(),
    );

    assert_eq!(blober_1.hash, expected_digest.as_ref());

    // Move to the next slot.
    context.warp_to_slot(2).unwrap();

    // Hash source account again.
    {
        let transaction = Transaction::new_signed_with_payer(
            &[Instruction {
                program_id,
                accounts: accounts::FinalizeBlob {
                    blober,
                    blob,
                    payer: context.payer.pubkey(),
                }
                .to_account_metas(None),
                data: instruction::FinalizeBlob {}.data(),
            }],
            Some(&context.payer.pubkey()),
            &[&context.payer],
            context.banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut context.banks_client, transaction)
            .await
            .expect_err("finalized same blob twice");
    }

    let blober_2 = context
        .banks_client
        .get_account(blober)
        .await
        .unwrap()
        .unwrap();

    let blober_2 =
        Blober::try_deserialize(&mut &blober_2.data[..]).expect("failed to deserialize blober");

    assert_eq!(blober_2.hash, expected_digest.as_ref());
}

#[tokio::test]
async fn hash_blober_itself() {
    let program_id = id();
    let system_program = solana_program::system_program::id();

    let program_test = ProgramTest::new("blober", program_id, processor!(test_entry));
    let (mut banks_client, payer, _) = program_test.start().await;

    let blober = find_blober_address(payer.pubkey(), "test");

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
            banks_client.get_latest_blockhash().await.unwrap(),
        );

        process_transaction(&mut banks_client, transaction)
            .await
            .expect("failed to create blober account");
    }

    // Hash source account.
    let transaction = Transaction::new_signed_with_payer(
        &[Instruction {
            program_id,
            accounts: accounts::FinalizeBlob {
                blober,
                blob: blober,
                payer: payer.pubkey(),
            }
            .to_account_metas(None),
            data: instruction::FinalizeBlob {}.data(),
        }],
        Some(&payer.pubkey()),
        &[&payer],
        banks_client.get_latest_blockhash().await.unwrap(),
    );

    // The transaction should fail because the source account is the same as the blober account.
    process_transaction(&mut banks_client, transaction)
        .await
        .unwrap_err();
}
