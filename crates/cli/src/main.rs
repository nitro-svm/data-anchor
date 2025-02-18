use nitro_da_cli::Options;
use nitro_da_client::BloberClientResult;
use tracing::error;
use tracing_subscriber::EnvFilter;

#[tokio::main]
pub async fn main() -> BloberClientResult {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let options = Options::parse();

    options.run().await.inspect_err(|e| {
        error!("Failed to run command: {e}");
    })
}
