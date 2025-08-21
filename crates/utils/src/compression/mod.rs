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

    #[error("Unknown compression type")]
    UnknownCompressionType,

    #[error("Compression type mismatch expected: {0:?}, found: {1:?}")]
    CompressionTypeMismatch(CompressionType, CompressionType),

    #[error("No data to decompress")]
    NoDataToDecompress,

    #[cfg(feature = "async")]
    #[error("Tokio task error: {0}")]
    TokioTaskError(#[from] tokio::task::JoinError),
}

pub type DataAnchorCompressionResult<T = ()> = Result<T, DataAnchorCompressionError>;

pub trait DataAnchorCompression: Send + Sync {
    fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>>;
    fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>>;
}

#[derive(Clone, Copy, std::default::Default)]
pub enum CompressionType {
    NoCompression,
    #[default]
    Lz4Compression,
    Flate2Compression,
    ZstdCompression(ruzstd::encoding::CompressionLevel),
}

impl serde::Serialize for CompressionType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8((*self).into())
    }
}

impl<'de> serde::Deserialize<'de> for CompressionType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        CompressionType::try_from(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "borsh")]
impl borsh::BorshSerialize for CompressionType {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        writer.write_all(&[u8::from(*self)])
    }
}

#[cfg(feature = "borsh")]
impl borsh::BorshDeserialize for CompressionType {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        let mut buffer = [0u8; 1];
        reader.read_exact(&mut buffer)?;
        CompressionType::try_from(buffer[0])
            .map_err(|e| borsh::io::Error::new(borsh::io::ErrorKind::InvalidData, e))
    }
}

impl std::fmt::Debug for CompressionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCompression => write!(f, "NoCompression"),
            Self::Lz4Compression => write!(f, "Lz4Compression"),
            Self::Flate2Compression => write!(f, "Flate2Compression"),
            Self::ZstdCompression(level) => write!(f, "{:?}", ZstdCompression(*level)),
        }
    }
}

impl PartialEq for CompressionType {
    fn eq(&self, other: &Self) -> bool {
        use CompressionType::*;
        match (self, other) {
            (NoCompression, NoCompression)
            | (Lz4Compression, Lz4Compression)
            | (Flate2Compression, Flate2Compression) => true,
            (ZstdCompression(l), ZstdCompression(r)) => {
                use ruzstd::encoding::CompressionLevel::*;
                matches!(
                    (l, r),
                    (Uncompressed, Uncompressed)
                        | (Fastest, Fastest)
                        | (Default, Default)
                        | (Better, Better)
                        | (Best, Best)
                )
            }
            _ => false,
        }
    }
}

impl Eq for CompressionType {}

impl std::fmt::Display for CompressionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressionType::NoCompression => write!(f, "no_compression"),
            CompressionType::Lz4Compression => write!(f, "lz4_compression"),
            CompressionType::Flate2Compression => write!(f, "flate2_compression"),
            CompressionType::ZstdCompression(level) => write!(f, "{}", ZstdCompression(*level)),
        }
    }
}

const NO_COMPRESSION_BYTE: u8 = 0;
const LZ4_COMPRESSION_BYTE: u8 = 1;
const FLATE2_COMPRESSION_BYTE: u8 = 2;
const ZSTD_UNCOMPRESSED_BYTE: u8 = 3;
const ZSTD_FASTEST_BYTE: u8 = 4;
const ZSTD_DEFAULT_BYTE: u8 = 5;
const ZSTD_BETTER_BYTE: u8 = 6;
const ZSTD_BEST_BYTE: u8 = 7;

impl From<CompressionType> for u8 {
    fn from(value: CompressionType) -> Self {
        use CompressionType::*;
        match value {
            NoCompression => NO_COMPRESSION_BYTE,
            Lz4Compression => LZ4_COMPRESSION_BYTE,
            Flate2Compression => FLATE2_COMPRESSION_BYTE,
            ZstdCompression(level) => {
                use ruzstd::encoding::CompressionLevel::*;
                match level {
                    Uncompressed => ZSTD_UNCOMPRESSED_BYTE,
                    Fastest => ZSTD_FASTEST_BYTE,
                    Default => ZSTD_DEFAULT_BYTE,
                    Better => ZSTD_BETTER_BYTE,
                    Best => ZSTD_BEST_BYTE,
                }
            }
        }
    }
}

impl TryFrom<u8> for CompressionType {
    type Error = DataAnchorCompressionError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use CompressionType::*;
        use ruzstd::encoding::CompressionLevel::*;
        match value {
            NO_COMPRESSION_BYTE => Ok(NoCompression),
            LZ4_COMPRESSION_BYTE => Ok(Lz4Compression),
            FLATE2_COMPRESSION_BYTE => Ok(Flate2Compression),
            ZSTD_UNCOMPRESSED_BYTE => Ok(ZstdCompression(Uncompressed)),
            ZSTD_FASTEST_BYTE => Ok(ZstdCompression(Fastest)),
            ZSTD_DEFAULT_BYTE => Ok(ZstdCompression(Default)),
            ZSTD_BETTER_BYTE => Ok(ZstdCompression(Better)),
            ZSTD_BEST_BYTE => Ok(ZstdCompression(Best)),
            _ => Err(DataAnchorCompressionError::UnknownCompressionType),
        }
    }
}

impl CompressionType {
    /// Add a marker byte to the beginning of the data to indicate the compression type.
    pub fn mark(self, data: Vec<u8>) -> Vec<u8> {
        [[self.into()].to_vec(), data].concat()
    }

    /// Inspect the compression type from a byte slice.
    pub fn inspect(data: &[u8]) -> DataAnchorCompressionResult<Self> {
        let Some(compression_type_byte) = data.first() else {
            return Err(DataAnchorCompressionError::NoDataToDecompress);
        };

        CompressionType::try_from(*compression_type_byte)
    }

    /// Extract the compression type and data from the given byte slice.
    pub fn get_compression_and_data(data: &[u8]) -> DataAnchorCompressionResult<(Self, &[u8])> {
        let Some((compression_type_byte, data)) = data.split_first() else {
            return Err(DataAnchorCompressionError::NoDataToDecompress);
        };

        let compression_type = CompressionType::try_from(*compression_type_byte)?;

        Ok((compression_type, data))
    }

    /// Assert that the compression type matches the expected type.
    pub fn assert_compression_type<'a>(
        &self,
        data: &'a [u8],
    ) -> DataAnchorCompressionResult<&'a [u8]> {
        let (compression_type, data) = Self::get_compression_and_data(data)?;
        if compression_type != *self {
            return Err(DataAnchorCompressionError::CompressionTypeMismatch(
                *self,
                compression_type,
            ));
        }

        Ok(data)
    }
}

impl DataAnchorCompression for CompressionType {
    fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        match self {
            CompressionType::NoCompression => NoCompression.compress(data),
            CompressionType::Lz4Compression => Lz4Compression.compress(data),
            CompressionType::Flate2Compression => Flate2Compression.compress(data),
            CompressionType::ZstdCompression(level) => ZstdCompression(*level).compress(data),
        }
    }

    fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let compression_type = CompressionType::inspect(data)?;

        match compression_type {
            CompressionType::NoCompression => NoCompression.decompress(data),
            CompressionType::Lz4Compression => Lz4Compression.decompress(data),
            CompressionType::Flate2Compression => Flate2Compression.decompress(data),
            CompressionType::ZstdCompression(level) => ZstdCompression(level).decompress(data),
        }
    }
}

#[derive(Debug, Clone, Copy, std::default::Default)]
pub struct NoCompression;

impl DataAnchorCompression for NoCompression {
    fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        Ok(CompressionType::NoCompression.mark(data.to_vec()))
    }

    fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        Ok(CompressionType::NoCompression
            .assert_compression_type(data)?
            .to_vec())
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

impl std::fmt::Display for ZstdCompression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ruzstd::encoding::CompressionLevel;

        write!(
            f,
            "zstd_compression_{}",
            match self.0 {
                CompressionLevel::Uncompressed => "uncompressed",
                CompressionLevel::Fastest => "fastest",
                CompressionLevel::Default => "default",
                CompressionLevel::Better => "better",
                CompressionLevel::Best => "best",
            }
        )
    }
}

impl std::default::Default for ZstdCompression {
    fn default() -> Self {
        ZstdCompression(ruzstd::encoding::CompressionLevel::Fastest)
    }
}

impl DataAnchorCompression for ZstdCompression {
    fn compress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        Ok(CompressionType::ZstdCompression(self.0)
            .mark(ruzstd::encoding::compress_to_vec(data, self.0)))
    }

    fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let mut data = CompressionType::ZstdCompression(self.0).assert_compression_type(data)?;
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
        Ok(CompressionType::Lz4Compression.mark(lz4_flex::compress_prepend_size(data)))
    }

    fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        Ok(lz4_flex::decompress_size_prepended(
            CompressionType::Lz4Compression.assert_compression_type(data)?,
        )?)
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
            .map(|compressed_data| CompressionType::Flate2Compression.mark(compressed_data))
    }

    fn decompress(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let data = CompressionType::Flate2Compression.assert_compression_type(data)?;
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
    #[case::compression_type(CompressionType::default(), true)]
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
            assert!(
                compressed_data.len() < data.len() + 1,
                "Compressed data should be smaller than original data plus the compression type byte: {} >= {}",
                compressed_data.len(),
                data.len() + 1
            );
        } else {
            assert!(compressed_data.len() >= data.len());
        }
        let decompressed_data = compression.decompress(&compressed_data).unwrap();
        assert_eq!(decompressed_data, data);
    }
}
