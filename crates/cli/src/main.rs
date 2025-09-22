use data_anchor::Options;
use tracing::error;
use tracing_subscriber::EnvFilter;

#[tokio::main]
pub async fn main() -> Result<(), Box<data_anchor_client::DataAnchorClientError>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let options = Options::parse();

    options.run().await.map_err(|e| {
        error!("Failed to run command: {e}");
        Box::new(e)
    })
}
