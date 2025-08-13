#[derive(Debug, thiserror::Error)]
pub enum DataAnchorEncodingError {
    #[error("Postcard encoding error: {0}")]
    Postcard(#[from] postcard::Error),

    #[error("Bincode encoding error: {0}")]
    Bincode(#[from] bincode::Error),

    #[error("JSON encoding error: {0}")]
    Json(#[from] serde_json::Error),

    #[cfg(feature = "borsh")]
    #[error("Borsh encoding error: {0}")]
    Borsh(#[from] borsh::io::Error),
}

pub type DataAnchorEncodingResult<T = ()> = Result<T, DataAnchorEncodingError>;

#[cfg(not(feature = "borsh"))]
mod _no_borsh {

    pub trait Encodable: serde::ser::Serialize {}

    impl<T: serde::ser::Serialize> Encodable for T {}

    pub trait Decodable: serde::de::DeserializeOwned {}

    impl<T: serde::de::DeserializeOwned> Decodable for T {}
}

#[cfg(not(feature = "borsh"))]
pub use _no_borsh::*;

#[cfg(feature = "borsh")]
mod _with_borsh {
    pub trait Encodable: serde::ser::Serialize + borsh::BorshSerialize {}

    impl<T: serde::ser::Serialize + borsh::BorshSerialize> Encodable for T {}

    pub trait Decodable: serde::de::DeserializeOwned + borsh::BorshDeserialize {}

    impl<T: serde::de::DeserializeOwned + borsh::BorshDeserialize> Decodable for T {}
}

#[cfg(feature = "borsh")]
pub use _with_borsh::*;

pub trait DataAnchorEncoding {
    fn encode<T: Encodable>(data: &T) -> DataAnchorEncodingResult<Vec<u8>>;

    fn decode<T: Decodable>(data: &[u8]) -> DataAnchorEncodingResult<T>;
}

#[derive(Debug, Clone, Copy)]
pub struct Postcard;

pub use Postcard as Default;

impl DataAnchorEncoding for Postcard {
    fn encode<T: Encodable>(data: &T) -> DataAnchorEncodingResult<Vec<u8>> {
        Ok(postcard::to_allocvec(data)?)
    }

    fn decode<T: Decodable>(data: &[u8]) -> DataAnchorEncodingResult<T> {
        Ok(postcard::from_bytes(data)?)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Bincode;

impl DataAnchorEncoding for Bincode {
    fn encode<T: Encodable>(data: &T) -> DataAnchorEncodingResult<Vec<u8>> {
        Ok(bincode::serialize(data)?)
    }

    fn decode<T: Decodable>(data: &[u8]) -> DataAnchorEncodingResult<T> {
        Ok(bincode::deserialize(data)?)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Json;

impl DataAnchorEncoding for Json {
    fn encode<T: Encodable>(data: &T) -> DataAnchorEncodingResult<Vec<u8>> {
        Ok(serde_json::to_vec(data)?)
    }

    fn decode<T: Decodable>(data: &[u8]) -> DataAnchorEncodingResult<T> {
        Ok(serde_json::from_slice(data)?)
    }
}

#[cfg(feature = "borsh")]
#[derive(Debug, Clone, Copy)]
pub struct Borsh;

#[cfg(feature = "borsh")]
impl DataAnchorEncoding for Borsh {
    fn encode<T: Encodable>(data: &T) -> DataAnchorEncodingResult<Vec<u8>> {
        Ok(borsh::to_vec(data)?)
    }

    fn decode<T: Decodable>(data: &[u8]) -> DataAnchorEncodingResult<T> {
        Ok(borsh::from_slice(data)?)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[derive(
        Debug,
        PartialEq,
        serde::Serialize,
        serde::Deserialize,
        borsh::BorshSerialize,
        borsh::BorshDeserialize,
    )]
    pub struct TestStruct {
        pub field1: String,
        pub field2: u32,
    }

    #[rstest]
    #[case::string("Hello, World!".to_string())]
    #[case::bytes(vec![20, 30])]
    #[case::tuple((1, 2, 3))]
    #[case::bool(true)]
    #[case::json(TestStruct {
        field1: "Test".to_string(),
        field2: 42,
    })]
    fn test_encoding<T, E>(
        #[case] data: T,
        #[values(Default, Postcard, Bincode, Json, Borsh)] _encoding: E,
    ) where
        T: Encodable + Decodable + PartialEq + std::fmt::Debug,
        E: DataAnchorEncoding,
    {
        let encoded = E::encode(&data).unwrap();
        let decoded: T = E::decode(&encoded).unwrap();
        assert_eq!(data, decoded);
    }
}
