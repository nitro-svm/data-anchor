#[cfg(feature = "compression")]
pub mod compression;
pub mod encoding;

#[cfg(feature = "compression")]
mod wrapper {
    /// Utility functions for encoding and compression in Data Anchor.
    #[derive(Debug, thiserror::Error)]
    pub enum DataAnchorUtilsError {
        #[error(transparent)]
        CompressionError(#[from] crate::compression::DataAnchorCompressionError),
        #[error(transparent)]
        EncodingError(#[from] crate::encoding::DataAnchorEncodingError),
    }

    /// Result type for Data Anchor utilities, encapsulating potential errors.
    pub type DataAnchorUtilsResult<T = ()> = Result<T, DataAnchorUtilsError>;

    /// Utility functions for encoding and compression in Data Anchor.
    pub fn encode_and_compress<T>(
        encoding: &EncodingType,
        compression: &CompressionType,
        data: &T,
    ) -> DataAnchorUtilsResult<Vec<u8>>
    where
        T: crate::encoding::Encodable,
    {
        let encoded_data = encoding.encode(data)?;
        Ok(compression.compress(&encoded_data)?)
    }

    /// Utility function to decompress and decode data in Data Anchor.
    pub fn decompress_and_decode<T>(data: &[u8]) -> DataAnchorUtilsResult<T>
    where
        T: crate::encoding::Decodable,
    {
        let decompressed_data = CompressionType::default().decompress(data)?;
        Ok(EncodingType::default().decode(&decompressed_data)?)
    }

    #[cfg(feature = "async")]
    mod _async {
        use super::DataAnchorUtilsResult;
        use crate::{
            compression::{CompressionType, DataAnchorCompressionAsync},
            encoding::{DataAnchorEncoding, EncodingType},
        };

        /// Utility functions for encoding and compression in Data Anchor.
        pub async fn encode_and_compress_async<T>(
            encoding: &EncodingType,
            compression: &CompressionType,
            data: &T,
        ) -> DataAnchorUtilsResult<Vec<u8>>
        where
            T: crate::encoding::Encodable,
        {
            let encoded_data = encoding.encode(data)?;
            Ok(compression.compress_async(&encoded_data).await?)
        }

        /// Utility function to decompress and decode data in Data Anchor.
        pub async fn decompress_and_decode_async<T>(data: &[u8]) -> DataAnchorUtilsResult<T>
        where
            T: crate::encoding::Decodable,
        {
            let decompressed_data = CompressionType::default().decompress_async(data).await?;
            Ok(EncodingType::default().decode(&decompressed_data)?)
        }
    }

    #[cfg(feature = "async")]
    pub use _async::*;

    use crate::{
        compression::{CompressionType, DataAnchorCompression},
        encoding::{DataAnchorEncoding, EncodingType},
    };
}

#[cfg(feature = "compression")]
pub use wrapper::*;
