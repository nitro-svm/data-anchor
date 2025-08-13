#![doc = include_str!("../README.md")]

use std::{path::PathBuf, str::FromStr, sync::Arc};

use anchor_lang::prelude::Pubkey;
use benchmark::BenchmarkSubCommand;
use blob::BlobSubCommand;
use blober::BloberSubCommand;
use clap::{CommandFactory, Parser, Subcommand, error::ErrorKind};
use data_anchor_client::{BloberIdentifier, DataAnchorClient, DataAnchorClientResult};
use data_anchor_utils::{compression, encoding};
use formatting::OutputFormat;
use indexer::IndexerSubCommand;
use solana_cli_config::Config;
use solana_keypair::Keypair;
use solana_signer::{EncodableKey, Signer};
use tracing::trace;

mod benchmark;
mod blob;
mod blober;
mod formatting;
mod indexer;

const NAMESPACE_MISSING_MSG: &str = "Namespace is not set. Please provide a namespace using the --namespace flag or set the DATA_ANCHOR_NAMESPACE environment variable.";
const INDEXER_URL_MISSING_MSG: &str = "Indexer URL is not set. Please provide a URL using the --indexer-url flag or set the DATA_ANCHOR_INDEXER_URL environment variable.";

/// The CLI options for the Blober CLI client.
#[derive(Debug, Parser)]
#[command(version, about, long_about)]
struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// The program ID of the Blober program.
    #[arg(short, long, global = true, env = "DATA_ANCHOR_PROGRAM_ID")]
    pub program_id: Option<Pubkey>,

    /// The namespace to use to generate the blober PDA.
    #[arg(short, long, global = true, env = "DATA_ANCHOR_NAMESPACE")]
    pub namespace: Option<String>,

    /// The blober PDA to use instead of generating one from the namespace.
    #[arg(
        short,
        long,
        global = true,
        env = "DATA_ANCHOR_BLOBER_PDA",
        value_name = "BLOBER_PDA"
    )]
    pub blober_pda: Option<Pubkey>,

    /// The payer account to use for transactions.
    #[arg(short = 's', long, global = true, env = "DATA_ANCHOR_PAYER")]
    pub payer: Option<String>,

    /// The output format to use.
    #[arg(
        short,
        long,
        global = true,
        env = "DATA_ANCHOR_OUTPUT",
        value_enum,
        default_value_t = OutputFormat::Text
    )]
    pub output: OutputFormat,

    /// The URL of the indexer to use.
    #[arg(short, long, global = true, env = "DATA_ANCHOR_INDEXER_URL")]
    pub indexer_url: Option<String>,

    /// The API token for the indexer, if required.
    #[arg(
        long,
        global = true,
        env = "DATA_ANCHOR_INDEXER_API_TOKEN",
        hide_env_values = true
    )]
    pub indexer_api_token: Option<String>,

    /// The path to the Solana [`Config`] file.
    #[arg(
        short,
        long,
        global = true,
        env = "DATA_ANCHOR_SOLANA_CONFIG_FILE",
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

    fn payer_keypair(&self, config: &Config) -> String {
        if let Some(payer) = &self.payer {
            return payer.to_owned();
        }

        let Ok(path_to_config) = PathBuf::from_str(&self.config_file);

        let Some(directory) = path_to_config.parent() else {
            Self::exit_with_missing_arg("Failed to get the parent directory of the config file")
        };

        let path = directory.join(&config.keypair_path);

        let Some(path_str) = path.to_str() else {
            Self::exit_with_missing_arg("Failed to convert the keypair path to a string")
        };

        path_str.to_owned()
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
    blober_pda: BloberIdentifier,
    indexer_url: Option<String>,
    indexer_api_token: Option<String>,
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
        let payer_path = args.payer_keypair(&config);
        let payer = Arc::new(Keypair::read_from_file(payer_path).unwrap());
        trace!("Parsed options: {args:?} {config:?} {payer:?}");

        let program_id = args.program_id.unwrap_or(data_anchor_blober::id());

        let blober_pda = if let Some(blober_pda) = args.blober_pda {
            blober_pda.into()
        } else {
            let Some(nmsp) = args.namespace else {
                Cli::exit_with_missing_arg(NAMESPACE_MISSING_MSG);
            };

            (payer.pubkey(), nmsp).into()
        };

        Self {
            indexer_url: args.indexer_url,
            indexer_api_token: args.indexer_api_token,
            command: args.command,
            program_id,
            output: args.output,
            blober_pda,
            payer,
            config,
        }
    }

    /// Run the parsed CLI command.
    pub async fn run(self) -> DataAnchorClientResult {
        let output = match self.command {
            Command::Indexer(subcommand) => {
                let Some(indexer_url) = self.indexer_url else {
                    Cli::exit_with_missing_arg(INDEXER_URL_MISSING_MSG);
                };
                let client = DataAnchorClient::<encoding::Default, compression::Default>::builder()
                    .payer(self.payer.clone())
                    .program_id(self.program_id)
                    .indexer_from_url(&indexer_url, self.indexer_api_token.clone())
                    .await?
                    .build_with_config(self.config)
                    .await?;
                let client = Arc::new(client);

                subcommand
                    .run(
                        client.clone(),
                        self.blober_pda
                            .to_blober_address(self.program_id, self.payer.pubkey()),
                    )
                    .await
            }
            subcommand => {
                let Some(namespace) = &self.blober_pda.namespace() else {
                    Cli::exit_with_missing_arg(NAMESPACE_MISSING_MSG);
                };
                let client = DataAnchorClient::<encoding::Default, compression::Default>::builder()
                    .payer(self.payer.clone())
                    .program_id(self.program_id)
                    .build_with_config(self.config)
                    .await?;
                let client = Arc::new(client);

                match subcommand {
                    Command::Blob(subcommand) => subcommand.run(client.clone(), namespace).await,
                    Command::Blober(subcommand) => {
                        subcommand
                            .run(
                                client.clone(),
                                self.blober_pda,
                                self.program_id,
                                self.payer.pubkey(),
                            )
                            .await
                    }
                    Command::Benchmark(subcommand) => {
                        subcommand.run(client.clone(), namespace).await
                    }
                    _ => unreachable!("Indexer subcommands should have been handled above"),
                }
            }
        }?;

        println!("{}", output.serialize_output(self.output));

        Ok(())
    }
}
