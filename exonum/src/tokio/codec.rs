use bytes::BytesMut;
use byteorder::{LittleEndian, ByteOrder};
use tokio_io::codec::{Decoder, Encoder};

use std::io;

use messages::{HEADER_LENGTH, MessageBuffer, RawMessage};
use super::error::other_error;

pub const MAX_MESSAGE_LEN: usize = 1024 * 1024; // 1 MB

#[derive(Debug)]
pub struct MessagesCodec;

impl Decoder for MessagesCodec {
    type Item = RawMessage;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, io::Error> {
        // Read header
        if buf.len() < HEADER_LENGTH {
            return Ok(None);
        }
        // Check payload len
        let total_len = LittleEndian::read_u32(&buf[6..10]) as usize;
        if total_len > MAX_MESSAGE_LEN {
            return Err(other_error(format!(
                "Received message is too long: {}, maximum allowed length is {}",
                total_len,
                MAX_MESSAGE_LEN
            )));
        }

        // Read message
        if buf.len() >= total_len {
            let data = buf.split_to(total_len).to_vec();
            let raw = RawMessage::from(MessageBuffer::from_vec(data));
            return Ok(Some(raw));
        }
        return Ok(None);
    }
}

impl Encoder for MessagesCodec {
    type Item = RawMessage;
    type Error = io::Error;

    fn encode(&mut self, msg: Self::Item, buf: &mut BytesMut) -> io::Result<()> {
        buf.extend_from_slice(msg.as_ref().as_ref());
        Ok(())
    }
}
