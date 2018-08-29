// Copyright 2018 The Exonum Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Messages used in the Exonum consensus algorithm.
//!
//! Every message, unless stated otherwise, is checked by the same set of rules. The message is
//! ignored if it
//!     * is sent from a lower height than the current one
//!     * contains incorrect validator id
//!     * is signed with incorrect signature
//!
//! Specific nuances are described in each message documentation and typically consist of three
//! parts:
//!     * validation - additional checks before processing
//!     * processing - how message is processed and result of the processing
//!     * generation - in which cases message is generated

use bit_vec::BitVec;
use chrono::{DateTime, Utc};

use std::fmt::Debug;
use std::net::SocketAddr;
use std::borrow::Cow;

use failure;

use super::{TransactionFromSet, Message, RawTransaction, BinaryForm,
            SignedMessage, BinaryFormSerialize};
use blockchain::{self, Transaction};
use crypto::{Hash, PublicKey, SecretKey, CryptoHash};
use helpers::{Height, Round, ValidatorId};
use storage::{Database, MemoryDB, ProofListIndex, StorageValue};

#[doc(hidden)]
/// TransactionsResponse size with zero transactions inside.
pub const TRANSACTION_RESPONSE_EMPTY_SIZE: usize = 261;

#[doc(hidden)]
/// RawTransaction size with zero transactions payload.
pub const RAW_TRANSACTION_EMPTY_SIZE: usize = 0;

encoding_struct! {
    /// Connect to a node.
    ///
    /// ### Validation
    /// The message is ignored if its time is earlier than in the previous
    /// `Connect` message received from the same peer.
    ///
    /// ### Processing
    /// Connect to the peer.
    ///
    /// ### Generation
    /// A node sends `Connect` message to all known addresses during
    /// initialization. Additionally, the node responds by its own `Connect`
    /// message after receiving `node::Event::Connected`.
    struct Connect {
        /// The node's address.
        addr: SocketAddr,
        /// Time when the message was created.
        time: DateTime<Utc>,
        /// String containing information about this node including Exonum, Rust and OS versions.
        user_agent: &str,
    }

}
encoding_struct! {
    /// Current node status.
    ///
    /// ### Validation
    /// The message is ignored if its signature is incorrect or its `height` is
    /// lower than a node's height.
    ///
    /// ### Processing
    /// If the message's `height` number is bigger than a node's one, then
    /// `BlockRequest` with current node's height is sent in reply.
    ///
    /// ### Generation
    /// `Status` message is broadcast regularly with the timeout controlled by
    /// `blockchain::ConsensusConfig::status_timeout`. Also, it is broadcast
    /// after accepting a new block.
    struct Status {
        /// The height to which the message is related.
        height: Height,
        /// Hash of the last committed block.
        last_hash: &Hash,
    }
}
encoding_struct! {
    /// Proposal for a new block.
    ///
    /// ### Validation
    /// The message is ignored if it
    ///     * contains incorrect `prev_hash`
    ///     * is sent by non-leader
    ///     * contains already committed transactions
    ///     * is already known
    ///
    /// ### Processing
    /// If the message contains unknown transactions, then `TransactionsRequest`
    /// is sent in reply.  Otherwise `Prevote` is broadcast.
    ///
    /// ### Generation
    /// A node broadcasts `Propose` if it is a leader and is not locked for a
    /// different proposal. Also `Propose` can be sent as response to
    /// `ProposeRequest`.
    struct Propose {
        /// The validator id.
        validator: ValidatorId,
        /// The height to which the message is related.
        height: Height,
        /// The round to which the message is related.
        round: Round,
        /// Hash of the previous block.
        prev_hash: &Hash,
        /// The list of transactions to include in the next block.
        transactions: &[Hash],
    }
}
encoding_struct! {
    /// Pre-vote for a new block.
    ///
    /// ### Validation
    /// A node panics if it has already sent a different `Prevote` for the same
    /// round.
    ///
    /// ### Processing
    /// Pre-vote is added to the list of known votes for the same proposal.  If
    /// `locked_round` number from the message is bigger than in a node state,
    /// then a node replies with `PrevotesRequest`.  If there are unknown
    /// transactions in the propose specified by `propose_hash`,
    /// `TransactionsRequest` is sent in reply.  Otherwise if all transactions
    /// are known and there are +2/3 pre-votes, then a node is locked to that
    /// proposal and `Precommit` is broadcast.
    ///
    /// ### Generation
    /// A node broadcasts `Prevote` in response to `Propose` when it has
    /// received all the transactions.
    struct Prevote {
        /// The validator id.
        validator: ValidatorId,
        /// The height to which the message is related.
        height: Height,
        /// The round to which the message is related.
        round: Round,
        /// Hash of the corresponding `Propose`.
        propose_hash: &Hash,
        /// Locked round.
        locked_round: Round,
    }
}
encoding_struct! {
    /// Pre-commit for a proposal.
    ///
    /// ### Validation
    /// A node panics if it has already sent a different `Precommit` for the
    /// same round.
    ///
    /// ### Processing
    /// Pre-commit is added to the list of known pre-commits.  If a proposal is
    /// unknown to the node, `ProposeRequest` is sent in reply.  If `round`
    /// number from the message is bigger than a node's "locked round", then a
    /// node replies with `PrevotesRequest`.  If there are unknown transactions,
    /// then `TransactionsRequest` is sent in reply.  If a validator receives
    /// +2/3 precommits for the same proposal with the same block_hash, then
    /// block is executed and `Status` is broadcast.
    ///
    /// ### Generation
    /// A node broadcasts `Precommit` in response to `Prevote` if there are +2/3
    /// pre-votes and no unknown transactions.
    struct Precommit {
        /// The validator id.
        validator: ValidatorId,
        /// The height to which the message is related.
        height: Height,
        /// The round to which the message is related.
        round: Round,
        /// Hash of the corresponding `Propose`.
        propose_hash: &Hash,
        /// Hash of the new block.
        block_hash: &Hash,
        /// Time of the `Precommit`.
        time: DateTime<Utc>,
    }
}
encoding_struct! {
    /// Information about a block.
    ///
    /// ### Validation
    /// The message is ignored if
    ///     * its `to` field corresponds to a different node
    ///     * the `block`, `transaction` and `precommits` fields cannot be
    ///     parsed or verified
    ///
    /// ### Processing
    /// The block is added to the blockchain.
    ///
    /// ### Generation
    /// The message is sent as response to `BlockRequest`.
    struct BlockResponse {
        /// Public key of the recipient.
        to: &PublicKey,
        /// Block header.
        block: blockchain::Block,
        /// List of pre-commits.
        precommits: Vec<Vec<u8>>,
        /// List of the transaction hashes.
        transactions: &[Hash],
    }
}
encoding_struct! {

    /// Information about the transactions.
    ///
    /// ### Validation
    /// The message is ignored if
    ///     * its `to` field corresponds to a different node
    ///     * the `transactions` field cannot be parsed or verified
    ///
    /// ### Processing
    /// Returns information about the transactions requested by the hash.
    ///
    /// ### Generation
    /// The message is sent as response to `TransactionsRequest`.
    struct TransactionsResponse {
        /// Public key of the recipient.
        to: &PublicKey,
        /// List of the transactions.
        transactions: Vec<Vec<u8>>,
    }

}
encoding_struct! {
    /// Request for the `Propose`.
    ///
    /// ### Validation
    /// The message is ignored if its `height` is not equal to the node's
    /// height.
    ///
    /// ### Processing
    /// `Propose` is sent as the response.
    ///
    /// ### Generation
    /// A node can send `ProposeRequest` during `Precommit` and `Prevote`
    /// handling.
    struct ProposeRequest {
        /// Public key of the recipient.
        to: &PublicKey,
        /// The height to which the message is related.
        height: Height,
        /// Hash of the `Propose`.
        propose_hash: &Hash,
    }
}
encoding_struct! {
    /// Request for transactions by hash.
    ///
    /// ### Processing
    /// Requested transactions are sent to the recipient.
    ///
    /// ### Generation
    /// This message can be sent during `Propose`, `Prevote` and `Precommit`
    /// handling.
    struct TransactionsRequest {
        /// Public key of the recipient.
        to: &PublicKey,
        /// The list of the transaction hashes.
        txs: &[Hash],
    }
}
encoding_struct! {
    /// Request for pre-votes.
    ///
    /// ### Validation
    /// The message is ignored if its `height` is not equal to the node's
    /// height.
    ///
    /// ### Processing
    /// The requested pre-votes are sent to the recipient.
    ///
    /// ### Generation
    /// This message can be sent during `Prevote` and `Precommit` handling.
    struct PrevotesRequest {
        /// Public key of the recipient.
        to: &PublicKey,
        /// The height to which the message is related.
        height: Height,
        /// The round to which the message is related.
        round: Round,
        /// Hash of the `Propose`.
        propose_hash: &Hash,
        /// The list of validators that send pre-votes.
        validators: BitVec,
    }
}
encoding_struct! {
    /// Request connected peers from a node.
    ///
    /// ### Validation
    /// Request is considered valid if the sender of the message on the network
    /// level corresponds to the `from` field.
    ///
    /// ### Processing
    /// Peer `Connect` messages are sent to the recipient.
    ///
    /// ### Generation
    /// `PeersRequest` message is sent regularly with the timeout controlled by
    /// `blockchain::ConsensusConfig::peers_timeout`.
    struct PeersRequest {
        /// Public key of the recipient.
        to: &PublicKey,
    }
}
encoding_struct! {
    /// Request for the block with the given `height`.
    ///
    /// ### Validation
    /// The message is ignored if its `height` is bigger than the node's one.
    ///
    /// ### Processing
    /// `BlockResponse` message is sent as the response.
    ///
    /// ### Generation
    /// This message can be sent during `Status` processing.

    struct BlockRequest {
        /// Public key of the recipient.
        to: & PublicKey,
        /// The height to which the message is related.
        height: Height,
    }
}

impl BlockResponse {
    /// Verify Merkle root of transactions in the block.
    pub fn verify_tx_hash(&self) -> bool {
        let db = MemoryDB::new();
        let mut fork = db.fork();
        let mut index = ProofListIndex::new("verify_tx_hash", &mut fork);
        index.extend(self.transactions().iter().cloned());
        let tx_hashes = index.merkle_root();
        tx_hashes == *self.block().tx_hash()
    }
}

impl Precommit {
    /// Verify precommit's signature and return it's safer wrapper
    pub(crate) fn verify_precommit(
        buffer: Vec<u8>,
    ) -> Result<Message<Precommit>, ::failure::Error> {
        unimplemented!()
//        let signed = SignedMessage::verify_buffer(buffer)?;
//        signed.into_message().map_into::<Precommit>()
    }
}

/// Full message constraints list.
#[doc(hidden)]
pub trait ProtocolMessage: Debug + Clone + BinaryForm
{
    fn message_type() -> (u8, u8);
    ///Trying to convert `Protocol` to concrete message,
    ///if ok returns message `Message<Self>` if fails, returns `Protocol` back.
    fn try_from(p: Protocol) -> Result<Message<Self>, Protocol>;

    fn into_protocol(this: Message<Self>) -> Protocol;

}

/// Implement Exonum message protocol.
///
/// Protocol should be described according to format:
/// ```
/// $SignedMessage => $ProtocolName {
///     $($cls_num => $MessageClass {
///         $(
///         $MessageType = $typ_num
///         )+
///     }
///     )+
/// }
/// ```
/// where:
/// `$ProtocolName` is an name of protocol which is described by this schema,
///     enum with same name will be created. All message classes are encapsulate by this enum;
/// `$MessageClass` is a module name which is designed to handle messages,
///     enum with same name will be crated. All message within `MessageClass` should be unique;
/// `$MessageType` is a concrete messages within some `$MessageClass`;
/// `$SignedMessage` is a typename of `SignedMessage` from which $ProtocolName could be created;
/// `$cls_num` and `$typ_num` is a constant which represent
///     message class and message type respectively
///
/// Each `$MessageType` should implement `Clone` and `Debug`.
///
macro_rules! impl_protocol {
    ($signed_message:ident => $proto_name:ident{
        $($cls_num:expr => $cls:ident{
            $( $typ:ident = $typ_num:expr),+ $(,)*
        } $(,)*)+
    }
    ) => {

        $(
            #[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
            pub enum $cls {
            $(
                $typ(Message<$typ>)
            ),+
            }

            $(

            impl ProtocolMessage for $typ {
                fn message_type() -> (u8, u8) {
                    ($cls_num, $typ_num)
                }

                fn try_from(p: $proto_name) -> Result<Message<Self>,Protocol> {
                    match p {
                        $proto_name::$cls($cls::$typ(s)) => Ok(s),
                        p => Err(p)
                    }
                }

                fn into_protocol(this: Message<Self>) -> Protocol {
                    $proto_name::$cls($cls::$typ(this))
                }
            }
            )+
        )+

        #[derive(PartialEq, Eq, Debug, Clone)]
        pub enum $proto_name {
            $(
                $cls($cls)
            ),+
        }

        impl $proto_name {
            pub fn deserialize(message: SignedMessage) -> Result<Protocol, failure::Error> {
            use $crate::events::error::into_failure;
                match message.message_class() {
                    $($cls_num =>
                        match message.message_type() {
                            $($typ_num =>{
                                let payload = $typ::deserialize(message.payload())
                                                .map_err(into_failure)?;
                                let message = Message::new(payload, message);
                                Ok($proto_name::$cls($cls::$typ(message)))
                            }),+
                            _ => bail!("Not found message with this type {}", message.message_type())
                        }
                    ),+
                    _ => bail!("Not found message with this class {}", message.message_class())
                }
            }

            pub fn signed_message(&self) -> &SignedMessage {
                match *self {
                    $(
                        $proto_name::$cls(ref c) => {
                            match *c {
                                $(
                                    $cls::$typ(ref t) => {
                                        t.signed_message()
                                    }
                                ),+
                            }
                        }
                    ),+
                }
            }
        }
    };
}

impl_protocol!{
    SignedMessage => Protocol {
        0 => Service {
            RawTransaction = 0,
            Connect = 1,
            Status = 2,
        },
        1 => Consensus {
            Precommit = 0,
            Propose = 1,
            Prevote = 2,
        },
        3 => Responses {
            TransactionsResponse = 0,
            BlockResponse = 1
        },
        3 => Requests {
            ProposeRequest = 0,
            TransactionsRequest = 1,
            PrevotesRequest = 2,
            PeersRequest = 3,
            BlockRequest = 4,
        },

    }
}


impl Protocol {

    /// Creates new protocol message.
    ///
    /// # Panics
    ///
    /// On serialization fail this method can panic.
    pub fn new<T: ProtocolMessage>(message: T, author: PublicKey, secret_key: &SecretKey)
                                -> Protocol {
        let signed = SignedMessage::new(message, author, secret_key);
        Self::deserialize(signed).expect("Couldn't deserialize newly created message.")
    }

    /// Creates new protocol message.
    /// Return concrete `Message<T>`
    ///
    /// # Panics
    ///
    /// On serialization fail this method can panic.
    pub fn concrete<T: ProtocolMessage>(message: T, author: PublicKey, secret_key: &SecretKey)
                                   -> Message<T> {
        T::try_from(Self::new(message, author, secret_key))
            .expect("BUG: Newly created message matched not as transaction.")
    }

    /// Creates new raw transaction message.
    ///
    /// # Panics
    ///
    /// On serialization fail this method can panic.
    pub(crate) fn sign_tx<T>(
        transaction: T,
        service_id: u16,
        public_key: PublicKey,
        secret_key: &SecretKey
    ) -> Message<RawTransaction>
    where T: Into<TransactionFromSet<T>> + Transaction,
     {
        let data = transaction.into()
                                       .serialize()
                                       .expect("Couldn't serialize transaction");
        let raw_tx = RawTransaction::new(service_id, data);
        Self::new(raw_tx, public_key, secret_key)
            .try_into_transaction()
            .expect("BUG: Newly created message matched not as transaction.")
    }

    ///Trying to convert `Protocol` to `RawTransaction`,
    ///if ok returns message `Message<RawTransaction>` if fails, returns `Protocol` back.
    pub fn try_into_transaction(self) -> Result<Message<RawTransaction>,Protocol> {
        RawTransaction::try_from(self)
    }
}

impl Requests {
    /// Returns public key of the message recipient.
    pub fn to(&self) -> &PublicKey {
        match *self {
            Requests::ProposeRequest(ref msg) => msg.to(),
            Requests::TransactionsRequest(ref msg) => msg.to(),
            Requests::PrevotesRequest(ref msg) => msg.to(),
            Requests::PeersRequest(ref msg) => msg.to(),
            Requests::BlockRequest(ref msg) => msg.to(),
        }
    }

    /// Returns author public key of the message sender.
    pub fn author(&self) -> &PublicKey {
        match *self {
            Requests::ProposeRequest(ref msg) => msg.author(),
            Requests::TransactionsRequest(ref msg) => msg.author(),
            Requests::PrevotesRequest(ref msg) => msg.author(),
            Requests::PeersRequest(ref msg) => msg.author(),
            Requests::BlockRequest(ref msg) => msg.author(),
        }
    }
}

impl Consensus {

    /// Returns author public key of the message sender.
    pub fn author(&self) -> &PublicKey {
        match *self {
            Consensus::Propose(ref msg) => msg.author(),
            Consensus::Prevote(ref msg) => msg.author(),
            Consensus::Precommit(ref msg) => msg.author(),
        }
    }

    /// Returns validator id of the message sender.
    pub fn validator(&self) -> ValidatorId {
        match *self {
            Consensus::Propose(ref msg) => msg.validator(),
            Consensus::Prevote(ref msg) => msg.validator(),
            Consensus::Precommit(ref msg) => msg.validator(),
        }
    }

    /// Returns height of the message.
    pub fn height(&self) -> Height {
        match *self {
            Consensus::Propose(ref msg) => msg.height(),
            Consensus::Prevote(ref msg) => msg.height(),
            Consensus::Precommit(ref msg) => msg.height(),
        }
    }

    /// Returns round of the message.
    pub fn round(&self) -> Round {
        match *self {
            Consensus::Propose(ref msg) => msg.round(),
            Consensus::Prevote(ref msg) => msg.round(),
            Consensus::Precommit(ref msg) => msg.round(),
        }
    }
}

impl<T: ProtocolMessage> Into<Protocol> for Message<T> {
    fn into(self) -> Protocol {
        ProtocolMessage::into_protocol(self)
    }
}

impl StorageValue for Protocol {
    fn into_bytes(self) -> Vec<u8> {
        unimplemented!()
    }

    fn from_bytes(value: Cow<[u8]>) -> Self {
        unimplemented!()
    }
}

impl CryptoHash for Protocol {
    fn hash(&self) -> Hash {
        unimplemented!()
    }
}



impl<T: Transaction> From<T> for TransactionFromSet<T> {
    fn from(t:T) -> Self {
        unimplemented!()
    }
}