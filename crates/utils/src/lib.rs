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
    pub fn encode_and_compress<Encoding, Compression, T>(
        encoding: &Encoding,
        compression: &Compression,
        data: &T,
    ) -> DataAnchorUtilsResult<Vec<u8>>
    where
        Encoding: crate::encoding::DataAnchorEncoding,
        Compression: crate::compression::DataAnchorCompression,
        T: crate::encoding::Encodable,
    {
        let encoded_data = encoding.encode(data)?;
        Ok(compression.compress(&encoded_data)?)
    }

    /// Utility function to decompress and decode data in Data Anchor.
    pub fn decompress_and_decode<Encoding, T>(
        encoding: &Encoding,
        compression: &dyn crate::compression::DataAnchorCompression,
        data: &[u8],
    ) -> DataAnchorUtilsResult<T>
    where
        Encoding: crate::encoding::DataAnchorEncoding,
        T: crate::encoding::Decodable,
    {
        let decompressed_data = compression.decompress(data)?;
        Ok(encoding.decode(&decompressed_data)?)
    }

    #[cfg(feature = "async")]
    mod _async {
        use super::DataAnchorUtilsResult;

        /// Utility functions for encoding and compression in Data Anchor.
        pub async fn encode_and_compress_async<Encoding, Compression, T>(
            encoding: &Encoding,
            compression: &Compression,
            data: &T,
        ) -> DataAnchorUtilsResult<Vec<u8>>
        where
            Encoding: crate::encoding::DataAnchorEncoding,
            Compression: crate::compression::DataAnchorCompressionAsync,
            T: crate::encoding::Encodable,
        {
            let encoded_data = encoding.encode(data)?;
            Ok(compression.compress_async(&encoded_data).await?)
        }

        /// Utility function to decompress and decode data in Data Anchor.
        pub async fn decompress_and_decode_async<Encoding, Compression, T>(
            encoding: &Encoding,
            compression: &Compression,
            data: &[u8],
        ) -> DataAnchorUtilsResult<T>
        where
            Encoding: crate::encoding::DataAnchorEncoding,
            Compression: crate::compression::DataAnchorCompressionAsync,
            T: crate::encoding::Decodable,
        {
            let decompressed_data = compression.decompress_async(data).await?;
            Ok(encoding.decode(&decompressed_data)?)
        }
    }

    #[cfg(feature = "async")]
    pub use _async::*;
}

#[cfg(feature = "compression")]
pub use wrapper::*;
