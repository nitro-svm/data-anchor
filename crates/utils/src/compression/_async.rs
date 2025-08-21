use super::{DataAnchorCompression, DataAnchorCompressionResult};

#[async_trait::async_trait]
pub trait DataAnchorCompressionAsync:
    DataAnchorCompression + Default + Send + Sync + Clone
{
    async fn compress_async(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>>;
    async fn decompress_async(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>>;
}

#[async_trait::async_trait]
impl<T> DataAnchorCompressionAsync for T
where
    T: DataAnchorCompression + Default + Send + Sync + Clone + 'static,
{
    async fn compress_async(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let data = data.to_vec();
        let cloned = self.clone();
        tokio::task::spawn_blocking(move || cloned.compress(data.as_slice())).await?
    }

    async fn decompress_async(&self, data: &[u8]) -> DataAnchorCompressionResult<Vec<u8>> {
        let data = data.to_vec();
        let cloned = self.clone();
        tokio::task::spawn_blocking(move || cloned.decompress(data.as_slice())).await?
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::compression::*;

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
    #[tokio::test]
    async fn test_compression_decompression<C>(
        #[case] compression: C,
        #[case] should_be_compressed: bool,
        #[values(1, 24, 100, 1000)] size: usize,
    ) where
        C: DataAnchorCompressionAsync,
    {
        let data = vec![100; size];
        let compressed_data = compression.compress_async(&data).await.unwrap();
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
        let decompressed_data = compression
            .decompress_async(&compressed_data)
            .await
            .unwrap();
        assert_eq!(decompressed_data, data);
    }
}
