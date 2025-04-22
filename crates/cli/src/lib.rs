#![doc = include_str!("../README.md")]

use std::sync::Arc;

use ::blober::find_blober_address;
use benchmark::BenchmarkSubCommand;
use blob::BlobSubCommand;
use blober::BloberSubCommand;
use clap::{CommandFactory, Parser, Subcommand, error::ErrorKind};
use formatting::OutputFormat;
use indexer::IndexerSubCommand;
use nitro_da_client::{BloberClient, BloberClientResult};
use solana_cli_config::Config;
use solana_sdk::{
    pubkey::Pubkey,
    signature::Keypair,
    signer::{EncodableKey, Signer},
};
use tracing::trace;

mod benchmark;
mod blob;
mod blober;
mod formatting;
mod indexer;

/// The CLI options for the Blober CLI client.
#[derive(Debug, Parser)]
#[command(version, about, long_about)]
struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// The program ID of the Blober program.
    #[arg(short, long, global = true, env = "BLOBER_PROGRAM_ID")]
    pub program_id: Option<Pubkey>,

    /// The namespace to use to generate the blober PDA.
    #[arg(short, long, global = true, env = "BLOBER_NAMESPACE")]
    pub namespace: Option<String>,

    /// The payer account to use for transactions.
    #[arg(short = 's', long, global = true, env = "BLOBER_PAYER")]
    pub payer: Option<String>,

    /// The output format to use.
    #[arg(
        short,
        long,
        global = true,
        env = "BLOBER_OUTPUT",
        value_enum,
        default_value_t = OutputFormat::Text
    )]
    pub output: OutputFormat,

    /// The URL of the indexer to use.
    #[arg(short, long, global = true, env = "BLOBER_INDEXER_URL")]
    pub indexer_url: Option<String>,

    /// The path to the Solana [`Config`] file.
    #[arg(
        short,
        long,
        global = true,
        env = "BLOBER_SOLANA_CONFIG_FILE",
        default_value_t = solana_cli_config::CONFIG_FILE.as_ref().unwrap().clone()
    )]
    pub config_file: String,
}

impl Cli {
    fn exit_with_missing_arg(msg: &str) -> ! {
        Self::command()
            .error(ErrorKind::MissingRequiredArgument, msg)
            .exit()
    }
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
    /// Subcommands for benchmarking the blober.
    #[command(subcommand, visible_alias = "m")]
    Benchmark(BenchmarkSubCommand),
}

pub struct Options {
    command: Command,
    program_id: Pubkey,
    payer: Arc<Keypair>,
    indexer_url: Option<String>,
    namespace: String,
    config: Config,
    output: OutputFormat,
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

        let Some(namespace) = args.namespace else {
            Cli::exit_with_missing_arg(
                "Namespace is not set. Please provide a namespace using the --namespace flag or set the BLOBER_NAMESPACE environment variable.",
            );
        };

        let Some(program_id) = args.program_id else {
            Cli::exit_with_missing_arg(
                "Program ID is not set. Please provide a program ID using the --program-id flag or set the BLOBER_PROGRAM_ID environment variable.",
            );
        };

        Self {
            namespace,
            indexer_url: args.indexer_url,
            command: args.command,
            program_id,
            output: args.output,
            payer,
            config,
        }
    }

    /// Run the parsed CLI command.
    pub async fn run(self) -> BloberClientResult {
        let blober = find_blober_address(self.program_id, self.payer.pubkey(), &self.namespace);

        let builder = BloberClient::builder()
            .payer(self.payer.clone())
            .program_id(self.program_id);

        let client = if let Some(indexer_url) = self.indexer_url {
            builder
                .indexer_from_url(&indexer_url)
                .await?
                .build_with_config(self.config)
                .await?
        } else {
            builder.build_with_config(self.config).await?
        };

        let client = Arc::new(client);

        let output = match self.command {
            Command::Blober(subcommand) => {
                subcommand.run(client.clone(), blober, self.namespace).await
            }
            Command::Blob(subcommand) => subcommand.run(client.clone(), blober).await,
            Command::Indexer(subcommand) => subcommand.run(client.clone(), blober).await,
            Command::Benchmark(subcommand) => subcommand.run(client.clone(), blober).await,
        }?;

        println!("{}", output.serialize_output(self.output));

        Ok(())
    }
}
