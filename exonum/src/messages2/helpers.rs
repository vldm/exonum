use hex::{FromHex, ToHex};
use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};

use std::fmt::Display;

use encoding::Error;

/// Helper trait to define serialization format.
pub trait BinaryForm: Sized {
    /// Converts transaction into serialized form.
    fn serialize(&self) -> Result<Vec<u8>, Error>;

    /// Converts a serialized byte array into a transaction.
    fn deserialize(buffer: &[u8]) -> Result<Self, Error>;
}

/// Uses `ToHex`/`FromHex` to serialize arbitrary type `T` as
/// hex string rather than real Serde::serialize.
pub(crate) struct HexStringRepresentation;

impl HexStringRepresentation {
    pub(crate) fn serialize<S, T>(message: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: ToHex,
    {
        let mut hex_string = String::new();
        message
            .write_hex(&mut hex_string)
            .map_err(ser::Error::custom)?;
        <String as Serialize>::serialize(&hex_string, serializer)
    }

    pub(crate) fn deserialize<'a, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'a>,
        T: FromHex,
        <T as FromHex>::Error: Display,
    {
        let hex_string = <String as Deserialize>::deserialize(deserializer)?;
        FromHex::from_hex(&hex_string).map_err(de::Error::custom)
    }
}
