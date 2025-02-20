use std::{path::PathBuf, sync::Arc};

use clap::{Args, Parser};
use nitro_da_client::{BloberClient, BloberClientResult};
use solana_sdk::pubkey::Pubkey;
use tokio::io::AsyncWriteExt;
use tracing::instrument;

#[derive(Debug, Parser)]
pub enum IndexerSubCommand {
    /// Get blobs for a given slot.
    #[command(visible_alias = "b")]
    Blobs(SlotArgs),
    /// Get compound proof for a given slot.
    #[command(visible_alias = "p")]
    Proofs(SlotArgs),
}

#[derive(Debug, Args)]
pub struct SlotArgs {
    /// The slot to query.
    pub slot: u64,

    /// The file to store the output into.
    #[arg(short, long)]
    pub output: PathBuf,
}

impl IndexerSubCommand {
    #[instrument(skip(client), level = "debug")]
    pub async fn run(&self, client: Arc<BloberClient>, blober: Pubkey) -> BloberClientResult {
        match self {
            IndexerSubCommand::Blobs(SlotArgs { slot, output }) => {
                let result = client.get_blobs(*slot, blober).await?;
                println!("Storing blobs for slot {slot} at {output:?}");
                let res: Result<(), Box<dyn std::error::Error>> = async move {
                    let file = tokio::fs::File::create(output).await?;
                    let mut writer = tokio::io::BufWriter::new(file);
                    for (i, blob) in result.into_iter().enumerate() {
                        writer.write_all(format!("{i}\n").as_bytes()).await?;
                        writer.write_all(blob.as_ref()).await?;
                    }
                    writer.flush().await?;
                    Ok(())
                }
                .await;
                match res {
                    Ok(_) => {
                        println!("Blobs for slot {slot} stored at {output:?}");
                    }
                    Err(e) => {
                        eprintln!("Error writing blobs: {e}");
                    }
                }
            }
            IndexerSubCommand::Proofs(SlotArgs { slot, output }) => {
                let result = client.get_slot_proof(*slot, blober).await?;
                println!("Storing proof for slot {slot} at {output:?}");
                let res: Result<(), Box<dyn std::error::Error>> = async move {
                    tokio::fs::write(output, serde_json::to_string(&result)?).await?;
                    Ok(())
                }
                .await;
                match res {
                    Ok(_) => {
                        println!("Proof for slot {slot} stored at {output:?}");
                    }
                    Err(e) => {
                        eprintln!("Error writing proof: {e}");
                    }
                }
            }
        }
        Ok(())
    }
}
