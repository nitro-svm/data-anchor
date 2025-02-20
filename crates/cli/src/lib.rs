#![doc = include_str!("../README.md")]

use std::sync::Arc;

use ::blober::find_blober_address;
use blob::BlobSubCommand;
use blober::BloberSubCommand;
use clap::{Parser, Subcommand};
use indexer::IndexerSubCommand;
use nitro_da_client::{BloberClient, BloberClientResult};
use solana_cli_config::Config;
use solana_sdk::{
    pubkey::Pubkey,
    signature::Keypair,
    signer::{EncodableKey, Signer},
};
use tracing::trace;

mod blob;
mod blober;
mod indexer;

/// The CLI options for the Blober CLI client.
#[derive(Debug, Parser)]
#[command(version, about, long_about)]
struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// The payer account to use for transactions.
    #[arg(short = 's', long)]
    pub payer: Option<String>,

    /// The program ID of the Blober program.
    #[arg(short, long)]
    pub program_id: Pubkey,

    /// The URL of the indexer to use.
    #[arg(short, long)]
    pub indexer_url: String,

    /// The namespace to use to generate the blober PDA.
    #[arg(short, long)]
    pub namespace: String,

    /// The path to the Solana [`Config`] file.
    #[arg(short, long, default_value_t = solana_cli_config::CONFIG_FILE.as_ref().unwrap().clone())]
    pub config_file: String,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Subcommands for managing the blober account.
    #[command(subcommand, visible_alias = "br")]
    Blober(BloberSubCommand),
    /// Subcommands for managing blobs.
    #[command(subcommand, visible_alias = "b")]
    Blob(BlobSubCommand),
    /// Subcommands for querying the indexer.
    #[command(subcommand, visible_alias = "i")]
    Indexer(IndexerSubCommand),
}

pub struct Options {
    command: Command,
    program_id: Pubkey,
    payer: Arc<Keypair>,
    indexer_url: String,
    namespace: String,
    config: Config,
}

impl Options {
    /// Parse the CLI options and load data from the Solana [`Config`] file and the payer
    /// [`Keypair`].
    pub fn parse() -> Self {
        trace!("Parsing options");
        let args = Cli::parse();
        let config = Config::load(&args.config_file).unwrap();
        let payer_path = args.payer.as_ref().unwrap_or(&config.keypair_path);
        let payer = Arc::new(Keypair::read_from_file(payer_path).unwrap());
        trace!("Parsed options: {args:?} {config:?} {payer:?}");

        Self {
            namespace: args.namespace,
            indexer_url: args.indexer_url,
            command: args.command,
            program_id: args.program_id,
            payer,
            config,
        }
    }

    /// Run the parsed CLI command.
    pub async fn run(self) -> BloberClientResult {
        let blober = find_blober_address(self.payer.pubkey(), &self.namespace);

        let client = Arc::new(
            BloberClient::builder()
                .payer(self.payer.clone())
                .program_id(self.program_id)
                .indexer_from_url(&self.indexer_url)
                .await?
                .build_with_config(self.config)
                .await?,
        );

        match self.command {
            Command::Blober(subcommand) => {
                subcommand
                    .run(client.clone(), blober, self.namespace)
                    .await?;
            }
            Command::Blob(subcommand) => {
                subcommand.run(client.clone(), blober).await?;
            }
            Command::Indexer(subcommand) => {
                subcommand.run(client.clone(), blober).await?;
            }
        }
        Ok(())
    }
}
