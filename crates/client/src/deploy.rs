use std::{fs::File, io::Read, path::PathBuf, sync::Arc};

use solana_bpf_loader_program::syscalls::create_program_runtime_environment_v1;
use solana_cli::{feature::CliFeatureStatus, program::calculate_max_chunk_size};
use solana_compute_budget::compute_budget::ComputeBudget;
use solana_program_runtime::invoke_context::InvokeContext;
use solana_rbpf::{elf::Executable, verifier::RequisiteVerifier};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::{config::RpcSimulateTransactionConfig, request::MAX_MULTIPLE_ACCOUNTS};
use solana_sdk::{
    account::Account,
    borsh1::try_from_slice_unchecked,
    bpf_loader_upgradeable,
    bpf_loader_upgradeable::UpgradeableLoaderState,
    compute_budget,
    compute_budget::ComputeBudgetInstruction,
    feature,
    feature_set::{FeatureSet, FEATURE_NAMES},
    message::Message,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};

use crate::{types::DeploymentError, BloberClient};

impl BloberClient {
    /// Deploys program via RPC Client
    // reference code https://github.com/anza-xyz/agave/blob/57a09f19f989228f1cfb195203cc60ea0120dd08/cli/src/program.rs#L1203
    #[allow(dead_code)]
    async fn deploy_solana_program(
        &self,
        payer: &Keypair,
        program_keypair: Option<Keypair>,
        program_authority: &Keypair,
        program_path: &str,
        max_len: Option<usize>,
    ) -> Result<(), DeploymentError> {
        let recent_blockhash = self
            .rpc_client
            .get_latest_blockhash()
            .await
            .map_err(|_e| DeploymentError::BlockHash)?;

        let program_keypair = if let Some(keypair) = program_keypair {
            keypair
        } else {
            get_default_program_keypair(&Some(program_path.to_string()))
        };

        let feature_set = fetch_feature_set(&self.rpc_client).await?;

        // Extract bytes from bytecode file indicated in program_path
        let program_data = read_and_verify_elf(program_path, feature_set)?;
        let program_len = program_data.len();
        let program_data_max_len = if let Some(len) = max_len {
            if program_len > len {
                return Err(DeploymentError::RentBalance(
                    "Max length specified not large enough to accommodate desired program"
                        .to_string(),
                ));
            }
            len
        } else {
            program_len
        };

        let buffer_keypair = Keypair::new();
        let minimum_rent_exempt_buffer_balance = self
            .rpc_client
            .get_minimum_balance_for_rent_exemption(UpgradeableLoaderState::size_of_programdata(
                program_data_max_len,
            ))
            .await
            .map_err(|e| {
                DeploymentError::RentBalance(format!(
                    "Failed to get rent balance for buffer account: {e}"
                ))
            })?;
        let buffer_program_data = vec![0; program_len];

        let initial_instructions = bpf_loader_upgradeable::create_buffer(
            &payer.pubkey(),
            &buffer_keypair.pubkey(),
            &program_authority.pubkey(),
            minimum_rent_exempt_buffer_balance,
            program_len,
        )
        .map_err(|e| {
            DeploymentError::Buffer(format!("Failed to create buffer instructions: {e}"))
        })?;

        let initial_message = Some(Message::new_with_blockhash(
            &initial_instructions,
            Some(&payer.pubkey()),
            &recent_blockhash,
        ));

        let create_msg = |offset: u32, bytes: Vec<u8>| {
            let instruction = bpf_loader_upgradeable::write(
                &buffer_keypair.pubkey(),
                &program_authority.pubkey(),
                offset,
                bytes,
            );

            Message::new_with_blockhash(&[instruction], Some(&payer.pubkey()), &recent_blockhash)
        };

        let mut write_messages = vec![];
        let chunk_size = calculate_max_chunk_size(&create_msg);
        for (chunk, i) in program_data.chunks(chunk_size).zip(0usize..) {
            let offset = i.saturating_mul(chunk_size);
            if chunk != &buffer_program_data[offset..offset.saturating_add(chunk.len())] {
                write_messages.push(create_msg(offset as u32, chunk.to_vec()));
            }
        }

        // Create and add final message
        let final_message = {
            let instructions = bpf_loader_upgradeable::deploy_with_max_program_len(
                &payer.pubkey(),
                &program_keypair.pubkey(),
                &buffer_keypair.pubkey(),
                &program_authority.pubkey(),
                self.rpc_client
                    .get_minimum_balance_for_rent_exemption(
                        UpgradeableLoaderState::size_of_program(),
                    )
                    .await
                    .map_err(|e| {
                        DeploymentError::RentBalance(format!(
                            "Failed to get rent balance for program account: {e}"
                        ))
                    })?,
                program_data_max_len,
            )
            .map_err(|e| {
                DeploymentError::Deploy(format!("Failed to create program deploy instruction: {e}"))
            })?;
            Some(Message::new_with_blockhash(
                &instructions,
                Some(&payer.pubkey()),
                &recent_blockhash,
            ))
        };

        self.send_deploy_messages(
            initial_message,
            write_messages,
            final_message,
            payer,
            &buffer_keypair,
            program_authority,
            &program_keypair,
        )
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_deploy_messages(
        &self,
        initial_message: Option<Message>,
        mut write_messages: Vec<Message>,
        final_message: Option<Message>,
        payer: &Keypair,
        buffer_keypair: &Keypair,
        program_authority: &Keypair,
        program_keypair: &Keypair,
    ) -> Result<Option<Signature>, DeploymentError> {
        if let Some(mut message) = initial_message {
            simulate_and_update_compute_unit_limit(&self.rpc_client, &mut message).await?;
            let mut initial_transaction = Transaction::new_unsigned(message.clone());
            let recent_blockhash = self
                .rpc_client
                .get_latest_blockhash()
                .await
                .map_err(|_e| DeploymentError::BlockHash)?;

            // Most of the initial_transaction combinations require both the fee-payer and new program
            // account to sign the transaction. One (transfer) only requires the fee-payer signature.
            // This check is to ensure signing does not fail on a KeypairPubkeyMismatch error from an
            // extraneous signature.
            if message.header.num_required_signatures == 2 {
                initial_transaction
                    .try_sign(&[payer, buffer_keypair], recent_blockhash)
                    .map_err(|e| {
                        DeploymentError::Deploy(format!("Failed to sign initial transaction: {e}"))
                    })?;
            } else {
                initial_transaction
                    .try_sign(&[payer], recent_blockhash)
                    .map_err(|e| {
                        DeploymentError::Deploy(format!("Failed to sign initial transaction: {e}"))
                    })?;
            }
            self.rpc_client
                .send_and_confirm_transaction_with_spinner(&initial_transaction)
                .await
                .map_err(|e| {
                    DeploymentError::Deploy(format!("Failed to send initial transactions: {e}"))
                })?;
        }

        if !write_messages.is_empty() {
            // Simulate the first write message to get the number of compute units
            // consumed and then reuse that value as the compute unit limit for all
            // write messages.
            {
                let mut message = write_messages[0].clone();
                if let UpdateComputeUnitLimitResult::UpdatedInstructionIndex(ix_index) =
                    simulate_and_update_compute_unit_limit(&self.rpc_client, &mut message).await?
                {
                    for msg in &mut write_messages {
                        // Write messages are all assumed to be identical except
                        // the program data being written. But just in case that
                        // assumption is broken, assert that we are only ever
                        // changing the instruction data for a compute budget
                        // instruction.
                        assert_eq!(msg.program_id(ix_index), Some(&compute_budget::id()));
                        msg.instructions[ix_index]
                            .data
                            .clone_from(&message.instructions[ix_index].data);
                    }
                }
            }

            for (idx, msg) in write_messages.iter().enumerate() {
                let mut write_tx = Transaction::new_unsigned(msg.clone());

                let recent_blockhash = self
                    .rpc_client
                    .get_latest_blockhash()
                    .await
                    .map_err(|_e| DeploymentError::BlockHash)?;

                write_tx
                    .try_sign(&[payer, program_authority], recent_blockhash)
                    .map_err(|e| {
                        DeploymentError::Deploy(format!(
                            "Failed to sign write transaction {idx}: {e}"
                        ))
                    })?;

                self.rpc_client.send_and_confirm_transaction_with_spinner(&write_tx).await
                        .map_err(|e| {
                            DeploymentError::Deploy(format!(
                                "Failed to send bytecode chunk transaction {idx} to write into buffer: {e}"
                            ))
                        })?;
            }
        }

        if let Some(mut message) = final_message {
            simulate_and_update_compute_unit_limit(&self.rpc_client, &mut message).await?;

            let mut final_tx = Transaction::new_unsigned(message);

            let recent_blockhash = self
                .rpc_client
                .get_latest_blockhash()
                .await
                .map_err(|_e| DeploymentError::BlockHash)?;
            final_tx
                .try_sign(
                    &[payer, program_keypair, program_authority],
                    recent_blockhash,
                )
                .map_err(|e| {
                    DeploymentError::Deploy(format!("Failed to sign final transaction: {e}"))
                })?;

            return Ok(Some(
                self.rpc_client
                    .send_and_confirm_transaction_with_spinner(&final_tx)
                    .await
                    .map_err(|e| {
                        DeploymentError::Deploy(format!("Deploying program failed: {e}"))
                    })?,
            ));
        }

        Ok(None)
    }
}

fn get_default_program_keypair(program_location: &Option<String>) -> Keypair {
    let program_keypair = {
        if let Some(program_location) = program_location {
            let mut keypair_file = PathBuf::new();
            keypair_file.push(program_location);
            let mut filename = keypair_file.file_stem().unwrap().to_os_string();
            filename.push("-keypair");
            keypair_file.set_file_name(filename);
            keypair_file.set_extension("json");
            if let Ok(keypair) = read_keypair_file(keypair_file.to_str().unwrap()) {
                keypair
            } else {
                Keypair::new()
            }
        } else {
            Keypair::new()
        }
    };
    program_keypair
}

fn read_and_verify_elf(
    program_location: &str,
    feature_set: FeatureSet,
) -> Result<Vec<u8>, DeploymentError> {
    let mut file = File::open(program_location)
        .map_err(|e| DeploymentError::Deploy(format!("Unable to open program file: {e}")))?;
    let mut program_data = Vec::new();
    file.read_to_end(&mut program_data)
        .map_err(|e| DeploymentError::Deploy(format!("Unable to read program file: {e}")))?;

    verify_elf(&program_data, feature_set)?;

    Ok(program_data)
}

fn verify_elf(program_data: &[u8], feature_set: FeatureSet) -> Result<(), DeploymentError> {
    // Verify the program
    let program_runtime_environment =
        create_program_runtime_environment_v1(&feature_set, &ComputeBudget::default(), true, false)
            .unwrap();
    let executable =
        Executable::<InvokeContext>::from_elf(program_data, Arc::new(program_runtime_environment))
            .map_err(|e| DeploymentError::Deploy(format!("ELF error: {e}")))?;

    executable
        .verify::<RequisiteVerifier>()
        .map_err(|e| DeploymentError::Deploy(format!("ELF error: {e}")))
}

async fn fetch_feature_set(rpc_client: &RpcClient) -> Result<FeatureSet, DeploymentError> {
    let mut feature_set = FeatureSet::default();
    for feature_ids in FEATURE_NAMES
        .keys()
        .cloned()
        .collect::<Vec<Pubkey>>()
        .chunks(MAX_MULTIPLE_ACCOUNTS)
    {
        rpc_client
            .get_multiple_accounts(feature_ids)
            .await
            .map_err(|e| {
                DeploymentError::Deploy(format!("Failed to retrieve feature id accounts: {e}"))
            })?
            .into_iter()
            .zip(feature_ids)
            .for_each(|(account, feature_id)| {
                let activation_slot = account.and_then(status_from_account);

                if let Some(CliFeatureStatus::Active(slot)) = activation_slot {
                    feature_set.activate(feature_id, slot);
                }
            });
    }

    Ok(feature_set)
}

fn status_from_account(account: Account) -> Option<CliFeatureStatus> {
    feature::from_account(&account).map(|feature| match feature.activated_at {
        None => CliFeatureStatus::Pending,
        Some(activation_slot) => CliFeatureStatus::Active(activation_slot),
    })
}

pub(crate) enum UpdateComputeUnitLimitResult {
    UpdatedInstructionIndex(usize),
    NoInstructionFound,
}

pub(crate) async fn simulate_and_update_compute_unit_limit(
    rpc_client: &RpcClient,
    message: &mut Message,
) -> Result<UpdateComputeUnitLimitResult, DeploymentError> {
    let Some(compute_unit_limit_ix_index) =
        message
            .instructions
            .iter()
            .enumerate()
            .find_map(|(ix_index, instruction)| {
                let ix_program_id = message.program_id(ix_index)?;
                if ix_program_id != &compute_budget::id() {
                    return None;
                }

                matches!(
                    try_from_slice_unchecked(&instruction.data),
                    Ok(ComputeBudgetInstruction::SetComputeUnitLimit(_))
                )
                .then_some(ix_index)
            })
    else {
        return Ok(UpdateComputeUnitLimitResult::NoInstructionFound);
    };

    let transaction = Transaction::new_unsigned(message.clone());
    let simulate_result = rpc_client
        .simulate_transaction_with_config(
            &transaction,
            RpcSimulateTransactionConfig {
                replace_recent_blockhash: true,
                commitment: Some(rpc_client.commitment()),
                ..RpcSimulateTransactionConfig::default()
            },
        )
        .await
        .map_err(|e| DeploymentError::Deploy(format!("Failed to simulate transaction {e}")))?
        .value;

    // Bail if the simulated transaction failed
    if let Some(err) = simulate_result.err {
        return Err(DeploymentError::Deploy(format!(
            "Failed to simulate transaction: {err}"
        )));
    }

    let units_consumed = simulate_result
        .units_consumed
        .expect("compute units unavailable");

    // Overwrite the compute unit limit instruction with the actual units consumed
    let compute_unit_limit = u32::try_from(units_consumed).map_err(|e| {
        DeploymentError::Deploy(format!("Failed to convert units consumed into u32: {e}"))
    })?;
    message.instructions[compute_unit_limit_ix_index].data =
        ComputeBudgetInstruction::set_compute_unit_limit(compute_unit_limit).data;

    Ok(UpdateComputeUnitLimitResult::UpdatedInstructionIndex(
        compute_unit_limit_ix_index,
    ))
}
