use serde::Serialize;
use serde_json::json;

use crate::{
    benchmark::{BenchmarkCommandOutput, write_measurements},
    blob::BlobCommandOutput,
    blober::BloberCommandOutput,
    indexer::IndexerCommandOutput,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    /// Output in regular human readable format.
    #[default]
    Text,
    /// Output in JSON format.
    Json,
    /// Output in pretty JSON format.
    JsonPretty,
    /// Output in CSV format.
    Csv,
}

#[derive(Debug, Serialize)]
pub enum CommandOutput {
    Blober(BloberCommandOutput),
    Blob(BlobCommandOutput),
    Indexer(IndexerCommandOutput),
    Benchmark(BenchmarkCommandOutput),
}

impl From<BloberCommandOutput> for CommandOutput {
    fn from(command: BloberCommandOutput) -> Self {
        CommandOutput::Blober(command)
    }
}

impl From<BlobCommandOutput> for CommandOutput {
    fn from(command: BlobCommandOutput) -> Self {
        CommandOutput::Blob(command)
    }
}

impl From<IndexerCommandOutput> for CommandOutput {
    fn from(command: IndexerCommandOutput) -> Self {
        CommandOutput::Indexer(command)
    }
}

impl From<BenchmarkCommandOutput> for CommandOutput {
    fn from(command: BenchmarkCommandOutput) -> Self {
        CommandOutput::Benchmark(command)
    }
}

impl std::fmt::Display for CommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandOutput::Blober(output) => write!(f, "{output}"),
            CommandOutput::Blob(output) => write!(f, "{output}"),
            CommandOutput::Indexer(output) => write!(f, "{output}"),
            CommandOutput::Benchmark(output) => write!(f, "{output}"),
        }
    }
}

impl CommandOutput {
    fn to_csv(&self) -> Result<String, Box<dyn std::error::Error>> {
        match self {
            CommandOutput::Blober(output) => {
                let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
                writer.serialize(output)?;
                Ok(String::from_utf8(writer.into_inner()?)?)
            }
            CommandOutput::Blob(output) => match output {
                BlobCommandOutput::Posting {
                    slot,
                    address,
                    signatures,
                    success,
                } => {
                    let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
                    writer.write_record(["slot", "address", "signatures", "success"])?;
                    writer.write_record(&[
                        format!("{slot}"),
                        format!("{address}"),
                        signatures
                            .iter()
                            .map(|sig| sig.to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                        format!("{success}"),
                    ])?;
                    Ok(String::from_utf8(writer.into_inner()?)?)
                }
                BlobCommandOutput::Fetching(vec) => {
                    let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
                    writer.write_record(["data"])?;
                    for blob in vec {
                        writer.write_record(&[hex::encode(blob)])?;
                    }
                    Ok(String::from_utf8(writer.into_inner()?)?)
                }
            },
            CommandOutput::Indexer(output) => match output {
                IndexerCommandOutput::Blobs(vec) => {
                    let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
                    writer.write_record(["data"])?;
                    for blob in vec {
                        writer.write_record(&[hex::encode(blob)])?;
                    }
                    Ok(String::from_utf8(writer.into_inner()?)?)
                }
                IndexerCommandOutput::Proofs(compound_proof) => {
                    let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
                    writer.serialize(compound_proof)?;
                    Ok(String::from_utf8(writer.into_inner()?)?)
                }
                IndexerCommandOutput::ZKProofs(proof_data) => {
                    let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
                    writer.serialize(proof_data)?;
                    Ok(String::from_utf8(writer.into_inner()?)?)
                }
                IndexerCommandOutput::ProofRequestStatus(request_id, status) => {
                    let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
                    writer.write_record(["request_id", "status"])?;
                    writer.write_record(&[request_id.clone(), format!("{status:?}")])?;
                    Ok(String::from_utf8(writer.into_inner()?)?)
                }
            },
            CommandOutput::Benchmark(output) => match output {
                BenchmarkCommandOutput::DataPath(path_buf) => {
                    let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
                    writer.write_record(["data_path"])?;
                    writer.write_record(&[format!("{}", path_buf.display())])?;
                    Ok(String::from_utf8(writer.into_inner()?)?)
                }
                BenchmarkCommandOutput::Measurements(vec) => {
                    Ok(write_measurements(vec.clone(), true)?)
                }
            },
        }
    }

    fn to_json(&self) -> Result<String, Box<dyn std::error::Error>> {
        let json_res = match self {
            CommandOutput::Blober(output) => serde_json::to_string(output),
            CommandOutput::Blob(output) => match output {
                BlobCommandOutput::Posting {
                    slot,
                    address,
                    signatures,
                    success,
                } => serde_json::to_string(&json!({
                    "slot": slot,
                    "address": address.to_string(),
                    "signatures": signatures.iter().map(|sig| sig.to_string()).collect::<Vec<_>>(),
                    "success": success,
                })),
                BlobCommandOutput::Fetching(vec) => {
                    let mut output = Vec::with_capacity(vec.len());
                    for blob in vec {
                        output.push(json!({
                            "data": hex::encode(blob),
                        }));
                    }
                    serde_json::to_string(&output)
                }
            },
            CommandOutput::Indexer(output) => match output {
                IndexerCommandOutput::Blobs(vec) => {
                    let mut output = Vec::with_capacity(vec.len());
                    for blob in vec {
                        output.push(json!({
                            "data": hex::encode(blob),
                        }));
                    }
                    serde_json::to_string(&output)
                }
                IndexerCommandOutput::Proofs(compound_proof) => {
                    serde_json::to_string(compound_proof)
                }
                IndexerCommandOutput::ZKProofs(proof_data) => serde_json::to_string(proof_data),
                IndexerCommandOutput::ProofRequestStatus(request_id, status) => {
                    serde_json::to_string(&json!({
                        "request_id": request_id,
                        "status": status,
                    }))
                }
            },
            CommandOutput::Benchmark(output) => serde_json::to_string(output),
        };

        Ok(json_res?)
    }

    fn to_json_pretty(&self) -> Result<String, Box<dyn std::error::Error>> {
        let json_res = match self {
            CommandOutput::Blober(output) => serde_json::to_string_pretty(output),
            CommandOutput::Blob(output) => match output {
                BlobCommandOutput::Posting {
                    slot,
                    address,
                    signatures,
                    success,
                } => serde_json::to_string_pretty(&json!({
                    "slot": slot,
                    "address": address.to_string(),
                    "signatures": signatures.iter().map(|sig| sig.to_string()).collect::<Vec<_>>(),
                    "success": success,
                })),
                BlobCommandOutput::Fetching(vec) => {
                    let mut output = Vec::with_capacity(vec.len());
                    for blob in vec {
                        output.push(json!({
                            "data": hex::encode(blob),
                        }));
                    }
                    serde_json::to_string_pretty(&output)
                }
            },
            CommandOutput::Indexer(output) => match output {
                IndexerCommandOutput::Blobs(vec) => {
                    let mut output = Vec::with_capacity(vec.len());
                    for blob in vec {
                        output.push(json!({
                            "data": hex::encode(blob),
                        }));
                    }
                    serde_json::to_string_pretty(&output)
                }
                IndexerCommandOutput::Proofs(compound_proof) => {
                    serde_json::to_string_pretty(compound_proof)
                }
                IndexerCommandOutput::ZKProofs(proof_data) => {
                    serde_json::to_string_pretty(proof_data)
                }
                IndexerCommandOutput::ProofRequestStatus(request_id, status) => {
                    serde_json::to_string_pretty(&json!({
                        "request_id": request_id,
                        "status": status,
                    }))
                }
            },
            CommandOutput::Benchmark(output) => serde_json::to_string_pretty(output),
        };

        Ok(json_res?)
    }

    /// Convert the command output to a string.
    pub fn serialize_output(&self, format: OutputFormat) -> String {
        let fallback = self.to_string();

        let output = match format {
            OutputFormat::Text => Ok(fallback.clone()),
            OutputFormat::Json => self.to_json().map_err(|_| ()),
            OutputFormat::JsonPretty => self.to_json_pretty().map_err(|_| ()),
            OutputFormat::Csv => self.to_csv().map_err(|_| ()),
        };

        output.unwrap_or(fallback)
    }
}
