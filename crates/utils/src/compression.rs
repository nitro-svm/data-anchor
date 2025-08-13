use std::io::{Read, Write};

use lz4_flex::{compress_prepend_size, decompress_size_prepended};

#[derive(Debug, thiserror::Error)]
pub enum DataAnchorCompressionError {
    #[error("Tokio task error: {0}")]
    TokioTaskError(#[from] tokio::task::JoinError),

    #[error("Zstd compression error: {0}")]
    ZstdCompressionError(std::io::Error),

    #[error("Lz4 compression error: {0}")]
    Lz4CompressionError(#[from] lz4_flex::block::DecompressError),

    #[error("Flate2 compression error: {0}")]
    Flate2CompressionError(std::io::Error),
}

pub type DataAnchorCompressionResult<T = ()> = Result<T, DataAnchorCompressionError>;

#[async_trait::async_trait]
pub trait DataAnchorCompression: std::default::Default + Send + Sync {
    type NewInput;

    fn new(new_input: Self::NewInput) -> Self;
    async fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>>;
    async fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>>;
}

#[derive(Debug, Clone, Copy, std::default::Default)]
pub struct NoCompression;

#[async_trait::async_trait]
impl DataAnchorCompression for NoCompression {
    type NewInput = ();

    fn new(_: Self::NewInput) -> Self {
        NoCompression
    }

    async fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        Ok(data.to_vec())
    }

    async fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        Ok(data.to_vec())
    }
}

pub struct ZstdCompression(pub i32);

impl std::default::Default for ZstdCompression {
    fn default() -> Self {
        ZstdCompression(6)
    }
}

pub use ZstdCompression as Default;

#[async_trait::async_trait]
impl DataAnchorCompression for ZstdCompression {
    type NewInput = i32;

    fn new(new_input: Self::NewInput) -> Self {
        ZstdCompression(new_input)
    }

    async fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let compression_level = self.0;
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || {
            zstd::encode_all(data.as_slice(), compression_level)
                .map_err(DataAnchorCompressionError::ZstdCompressionError)
        })
        .await?
    }

    async fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || {
            zstd::decode_all(data.as_slice())
                .map_err(DataAnchorCompressionError::ZstdCompressionError)
        })
        .await?
    }
}

#[derive(Debug, Clone, Copy, std::default::Default)]
pub struct Lz4Compression;

#[async_trait::async_trait]
impl DataAnchorCompression for Lz4Compression {
    type NewInput = ();

    fn new(_: Self::NewInput) -> Self {
        Lz4Compression
    }

    async fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || Ok(compress_prepend_size(&data))).await?
    }

    async fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || {
            decompress_size_prepended(&data)
                .map_err(DataAnchorCompressionError::Lz4CompressionError)
        })
        .await?
    }
}

#[derive(Debug, Clone, Copy, std::default::Default)]
pub struct Flate2Compression;

#[async_trait::async_trait]
impl DataAnchorCompression for Flate2Compression {
    type NewInput = ();

    fn new(_: Self::NewInput) -> Self {
        Flate2Compression
    }

    async fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder
                .write_all(&data)
                .map_err(DataAnchorCompressionError::Flate2CompressionError)?;
            encoder
                .finish()
                .map_err(DataAnchorCompressionError::Flate2CompressionError)
        })
        .await?
    }

    async fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut decoder = flate2::read::GzDecoder::new(data.as_slice());
            let mut decompressed_data = Vec::new();
            decoder
                .read_to_end(&mut decompressed_data)
                .map_err(DataAnchorCompressionError::Flate2CompressionError)?;
            Ok(decompressed_data)
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::no_compression(NoCompression, false)]
    #[case::default_compression(Default::default(), true)]
    #[case::zstd_compression(ZstdCompression::default(), true)]
    #[case::zstd_custom_compression(ZstdCompression(1), true)]
    #[case::lz4_compression(Lz4Compression, true)]
    #[case::flate2_compression(Flate2Compression, true)]
    #[tokio::test]
    async fn test_compression_decompression<C>(
        #[case] compression: C,
        #[case] should_be_compressed: bool,
        #[values(1, 24, 100, 1000)] size: usize,
    ) where
        C: DataAnchorCompression,
    {
        let data = vec![100; size];
        let compressed_data = compression.compress(&data).await.unwrap();
        // When size is less than 24, compression does not reduce size
        if should_be_compressed && size >= 24 {
            assert!(compressed_data.len() < data.len());
        } else {
            assert!(compressed_data.len() >= data.len());
        }
        let decompressed_data = compression.decompress(&compressed_data).await.unwrap();
        assert_eq!(decompressed_data, data);
    }
}
