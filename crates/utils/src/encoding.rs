#[derive(Debug, thiserror::Error)]
pub enum DataAnchorEncodingError {
    #[error("Postcard encoding error: {0}")]
    Postcard(#[from] postcard::Error),

    #[error("Bincode encoding error: {0}")]
    Bincode(#[from] bincode::Error),

    #[error("JSON encoding error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Unknown encoding type")]
    UnknownEncodingType,

    #[error("Encoding type mismatch expected: {0:?}, found: {1:?}")]
    EncodingTypeMismatch(EncodingType, EncodingType),

    #[error("No data to decode")]
    NoDataToDecode,

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
    fn encode<T: Encodable>(&self, data: &T) -> DataAnchorEncodingResult<Vec<u8>>;
    fn decode<T: Decodable>(&self, data: &[u8]) -> DataAnchorEncodingResult<T>;
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, std::default::Default, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
#[repr(u8)]
pub enum EncodingType {
    #[default]
    Postcard,
    Bincode,
    Json,
    #[cfg(feature = "borsh")]
    Borsh,
}

impl std::fmt::Display for EncodingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodingType::Postcard => write!(f, "postcard"),
            EncodingType::Bincode => write!(f, "bincode"),
            EncodingType::Json => write!(f, "json"),
            #[cfg(feature = "borsh")]
            EncodingType::Borsh => write!(f, "borsh"),
        }
    }
}

impl TryFrom<u8> for EncodingType {
    type Error = DataAnchorEncodingError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(EncodingType::Postcard),
            1 => Ok(EncodingType::Bincode),
            2 => Ok(EncodingType::Json),
            #[cfg(feature = "borsh")]
            3 => Ok(EncodingType::Borsh),
            _ => Err(DataAnchorEncodingError::UnknownEncodingType),
        }
    }
}

impl EncodingType {
    /// Add a marker byte to the beginning of the data to indicate the encoding type.
    pub fn mark(self, data: Vec<u8>) -> Vec<u8> {
        [[self as u8].to_vec(), data].concat()
    }

    /// Inspect encoding type from a byte slice.
    pub fn inspect(data: &[u8]) -> DataAnchorEncodingResult<Self> {
        let Some(encoding_type_byte) = data.first() else {
            return Err(DataAnchorEncodingError::NoDataToDecode);
        };

        EncodingType::try_from(*encoding_type_byte)
    }

    /// Extract the encoding type and the data from a byte slice.
    pub fn get_encoding_and_data(data: &[u8]) -> DataAnchorEncodingResult<(Self, &[u8])> {
        let Some((encoding_type_byte, data)) = data.split_first() else {
            return Err(DataAnchorEncodingError::NoDataToDecode);
        };

        let encoding_type = EncodingType::try_from(*encoding_type_byte)?;

        Ok((encoding_type, data))
    }

    /// Assert that the encoding type matches the expected type.
    pub fn assert_encoding_type<'a>(&self, data: &'a [u8]) -> DataAnchorEncodingResult<&'a [u8]> {
        let (encoding_type, data) = Self::get_encoding_and_data(data)?;
        if encoding_type != *self {
            return Err(DataAnchorEncodingError::EncodingTypeMismatch(
                *self,
                encoding_type,
            ));
        }
        Ok(data)
    }
}

impl DataAnchorEncoding for EncodingType {
    fn encode<T: Encodable>(&self, data: &T) -> DataAnchorEncodingResult<Vec<u8>> {
        match self {
            EncodingType::Postcard => Postcard.encode(data),
            EncodingType::Bincode => Bincode.encode(data),
            EncodingType::Json => Json.encode(data),
            #[cfg(feature = "borsh")]
            EncodingType::Borsh => Borsh.encode(data),
        }
    }

    fn decode<T: Decodable>(&self, data: &[u8]) -> DataAnchorEncodingResult<T> {
        let encoding_type = EncodingType::inspect(data)?;

        match encoding_type {
            EncodingType::Postcard => Postcard.decode(data),
            EncodingType::Bincode => Bincode.decode(data),
            EncodingType::Json => Json.decode(data),
            #[cfg(feature = "borsh")]
            EncodingType::Borsh => Borsh.decode(data),
        }
    }
}

#[derive(Debug, Clone, Copy, std::default::Default)]
pub struct Postcard;

pub use Postcard as Default;

impl DataAnchorEncoding for Postcard {
    fn encode<T: Encodable>(&self, data: &T) -> DataAnchorEncodingResult<Vec<u8>> {
        Ok(EncodingType::Postcard.mark(postcard::to_allocvec(data)?))
    }

    fn decode<T: Decodable>(&self, data: &[u8]) -> DataAnchorEncodingResult<T> {
        Ok(postcard::from_bytes(
            EncodingType::Postcard.assert_encoding_type(data)?,
        )?)
    }
}

#[derive(Debug, Clone, Copy, std::default::Default)]
pub struct Bincode;

impl DataAnchorEncoding for Bincode {
    fn encode<T: Encodable>(&self, data: &T) -> DataAnchorEncodingResult<Vec<u8>> {
        Ok(EncodingType::Bincode.mark(bincode::serialize(data)?))
    }

    fn decode<T: Decodable>(&self, data: &[u8]) -> DataAnchorEncodingResult<T> {
        Ok(bincode::deserialize(
            EncodingType::Bincode.assert_encoding_type(data)?,
        )?)
    }
}

#[derive(Debug, Clone, Copy, std::default::Default)]
pub struct Json;

impl DataAnchorEncoding for Json {
    fn encode<T: Encodable>(&self, data: &T) -> DataAnchorEncodingResult<Vec<u8>> {
        Ok(EncodingType::Json.mark(serde_json::to_vec(data)?))
    }

    fn decode<T: Decodable>(&self, data: &[u8]) -> DataAnchorEncodingResult<T> {
        Ok(serde_json::from_slice(
            EncodingType::Json.assert_encoding_type(data)?,
        )?)
    }
}

#[cfg(feature = "borsh")]
#[derive(Debug, Clone, Copy, std::default::Default)]
pub struct Borsh;

#[cfg(feature = "borsh")]
impl DataAnchorEncoding for Borsh {
    fn encode<T: Encodable>(&self, data: &T) -> DataAnchorEncodingResult<Vec<u8>> {
        Ok(EncodingType::Borsh.mark(borsh::to_vec(data)?))
    }

    fn decode<T: Decodable>(&self, data: &[u8]) -> DataAnchorEncodingResult<T> {
        Ok(borsh::from_slice(
            EncodingType::Borsh.assert_encoding_type(data)?,
        )?)
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
        #[values(Default, Postcard, Bincode, Json, Borsh, EncodingType::default())] encoding: E,
    ) where
        T: Encodable + Decodable + PartialEq + std::fmt::Debug,
        E: DataAnchorEncoding,
    {
        let encoded = encoding.encode(&data).unwrap();
        let decoded: T = encoding.decode(&encoded).unwrap();
        assert_eq!(data, decoded);
    }
}
