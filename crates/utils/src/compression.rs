use std::io::{Read, Write};

#[cfg(feature = "async")]
mod _async;

#[cfg(feature = "async")]
pub use _async::*;

#[derive(Debug, thiserror::Error)]
pub enum DataAnchorCompressionError {
    #[error("Zstd decoding error: {0}")]
    ZstdDecodingError(#[from] ruzstd::decoding::errors::FrameDecoderError),

    #[error("Zstd decoding error: {0}")]
    ZstdDecodingIoError(#[from] std::io::Error),

    #[error("Lz4 compression error: {0}")]
    Lz4CompressionError(#[from] lz4_flex::block::DecompressError),

    #[error("Flate2 compression error: {0}")]
    Flate2CompressionError(std::io::Error),

    #[cfg(feature = "async")]
    #[error("Tokio task error: {0}")]
    TokioTaskError(#[from] tokio::task::JoinError),
}

pub type DataAnchorCompressionResult<T = ()> = Result<T, DataAnchorCompressionError>;

pub trait DataAnchorCompression: Send + Sync {
    fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>>;
    fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>>;
}

#[derive(Debug, Clone, Copy, std::default::Default)]
pub struct NoCompression;

impl DataAnchorCompression for NoCompression {
    fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        Ok(data.to_vec())
    }

    fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        Ok(data.to_vec())
    }
}

#[derive(Clone, Copy)]
pub struct ZstdCompression(pub ruzstd::encoding::CompressionLevel);

impl std::fmt::Debug for ZstdCompression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ruzstd::encoding::CompressionLevel;

        f.debug_tuple("ZstdCompression")
            .field(match self.0 {
                CompressionLevel::Uncompressed => &"Uncompressed",
                CompressionLevel::Fastest => &"Fastest",
                CompressionLevel::Default => &"Default",
                CompressionLevel::Better => &"Better",
                CompressionLevel::Best => &"Best",
            })
            .finish()
    }
}

impl std::default::Default for ZstdCompression {
    fn default() -> Self {
        ZstdCompression(ruzstd::encoding::CompressionLevel::Fastest)
    }
}

impl DataAnchorCompression for ZstdCompression {
    fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        Ok(ruzstd::encoding::compress_to_vec(data, self.0))
    }

    fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let mut data = data;
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(&mut data)?;

        let mut result = Vec::new();
        decoder.read_to_end(&mut result)?;

        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, std::default::Default)]
pub struct Lz4Compression;

pub use Lz4Compression as Default;

impl DataAnchorCompression for Lz4Compression {
    fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        Ok(lz4_flex::compress_prepend_size(data))
    }

    fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        Ok(lz4_flex::decompress_size_prepended(data)?)
    }
}

#[derive(Debug, Clone, Copy, std::default::Default)]
pub struct Flate2Compression;

impl DataAnchorCompression for Flate2Compression {
    fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(data)
            .map_err(DataAnchorCompressionError::Flate2CompressionError)?;
        encoder
            .finish()
            .map_err(DataAnchorCompressionError::Flate2CompressionError)
    }

    fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let mut decoder = flate2::read::GzDecoder::new(data);
        let mut decompressed_data = Vec::new();
        decoder
            .read_to_end(&mut decompressed_data)
            .map_err(DataAnchorCompressionError::Flate2CompressionError)?;
        Ok(decompressed_data)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::no_compression(NoCompression, false)]
    #[case::default_compression(Default, true)]
    #[case::zstd_compression(ZstdCompression::default(), true)]
    #[case::zstd_custom_compression(
        ZstdCompression(ruzstd::encoding::CompressionLevel::Fastest),
        true
    )]
    #[case::lz4_compression(Lz4Compression, true)]
    #[case::flate2_compression(Flate2Compression, true)]
    fn test_compression_decompression<C>(
        #[case] compression: C,
        #[case] should_be_compressed: bool,
        #[values(1, 24, 100, 1000)] size: usize,
    ) where
        C: DataAnchorCompression,
    {
        let data = vec![100; size];
        let compressed_data = compression.compress(&data).unwrap();
        // When size is less than 24, compression does not reduce size
        if should_be_compressed && size >= 24 {
            assert!(compressed_data.len() < data.len());
        } else {
            assert!(compressed_data.len() >= data.len());
        }
        let decompressed_data = compression.decompress(&compressed_data).unwrap();
        assert_eq!(decompressed_data, data);
    }
}
