use std::borrow::Cow;

use failure::Error;
use serde::{Deserialize, Serialize};

use crypto::{
    self, hash, CryptoHash, Hash, PublicKey, SecretKey, Signature, PUBLIC_KEY_LENGTH,
    SIGNATURE_LENGTH,
};
use messages::Message;
use storage::StorageValue;

use super::protocol::{Protocol, ProtocolMessage};
use super::PROTOCOL_MAJOR_VERSION;

use encoding::serialize::encode_hex;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuthorisedMessage {
    pub version: u8,
    pub author: PublicKey,
    pub protocol: Protocol,
}

impl AuthorisedMessage {
    fn new<T: Into<Protocol>>(value: T, author: PublicKey) -> Result<Self, Error> {
        Ok(AuthorisedMessage {
            version: PROTOCOL_MAJOR_VERSION,
            author,
            protocol: value.into(),
        })
    }
}

/// Correct raw message that was deserialized and verifyied, from `UncheckedBuffer`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SignedMessage {
    pub(crate) authorised_message: AuthorisedMessage,
    pub(crate) signature: Signature,
}

impl SignedMessage {
    pub(crate) fn new<T: Into<Protocol>>(
        value: T,
        author: PublicKey,
        secret_key: &SecretKey,
    ) -> Result<SignedMessage, Error> {
        let authorised_message = AuthorisedMessage::new(value, author)?;
        let signature = Self::sign(&authorised_message, secret_key)?;

        Ok(SignedMessage {
            authorised_message,
            signature,
        })
    }

    /// Create `SignedMessage` wrapper from `UncheckedBuffer`.
    /// Checks binary format and signature.
    pub fn verify_buffer<T: AsRef<[u8]>>(buffer: T) -> Result<SignedMessage, Error> {
        // TODO: external serialization library shadows any knowledge about internal
        // binary representation.
        // Sodium verify/sign api allows to work only with raw buffer.
        // This two factors lead to additional `serialize` inside verify
        let buffer = buffer.as_ref();
        let message: SignedMessage = ::bincode::config().no_limit().deserialize(&buffer)?;
        Self::verify(
            &message.authorised_message,
            &message.signature,
            &message.authorised_message.author,
        )?;
        Ok(message)
    }

    /// Serialize safe wrapper into unchecked byte array.
    pub fn to_vec(&self) -> Vec<u8> {
        ::bincode::config()
            .no_limit()
            .serialize(&self)
            .expect("Could not serialize SignedMessage.")
    }

    /// Serialize message as hex encoded byte array
    pub fn to_hex_string(&self) -> String {
        encode_hex(&self.to_vec())
    }

    /// Converts signed message into root safe wrapper.
    pub fn into_message(self) -> Message {
        Message {
            payload: self.authorised_message.protocol.clone(),
            message: self,
        }
    }

    fn sign<T: Serialize>(val: &T, secret_key: &SecretKey) -> Result<Signature, Error> {
        // TODO: limit bincode max_message_length using config
        let full_buffer = ::bincode::config().no_limit().serialize(&val)?;
        let signature = crypto::sign(&full_buffer, secret_key);
        Ok(signature)
    }

    fn hash(&self) -> Hash {
        hash(&::bincode::config()
            .no_limit()
            .serialize(self)
            .expect("Expected serialize to work"))
    }

    fn verify<T: Serialize>(
        val: &T,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> Result<(), Error> {
        let full_buffer = ::bincode::config().no_limit().serialize(&val)?;
        if !crypto::verify(signature, &full_buffer, &public_key) {
            bail!("Can't verify message.");
        }
        Ok(())
    }
}

impl StorageValue for SignedMessage {
    fn into_bytes(self) -> Vec<u8> {
        self.to_vec()
    }

    fn from_bytes(value: Cow<[u8]>) -> Self {
        // TODO: remove signature validation (StorageValue is an internal trait)
        SignedMessage::verify_buffer(&value).unwrap()
    }
}

impl CryptoHash for SignedMessage {
    fn hash(&self) -> Hash {
        self.hash()
    }
}

impl<T: ProtocolMessage> StorageValue for Message<T> {
    fn into_bytes(self) -> Vec<u8> {
        self.message.to_vec()
    }

    fn from_bytes(value: Cow<[u8]>) -> Self {
        //TODO: Remove signature checks and type checks (Getting value from database should be safe)
        let message = SignedMessage::verify_buffer(&value).unwrap().into_message();
        message.map_into().unwrap()
    }
}

impl<T: ProtocolMessage> CryptoHash for Message<T> {
    fn hash(&self) -> Hash {
        unimplemented!()
    }
}