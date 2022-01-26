use bitflags::*;
use bytes::Bytes;
use num_derive::*;

bitflags! {
    /// Contact Header flags
    #[derive(Default)]
    pub(crate) struct ContactHeaderFlags : u8 {
        const CAN_TLS = 0x01;
    }
}

/// Message Types
#[derive(Debug, FromPrimitive)]
#[repr(u8)]
pub(crate) enum MessageType {
    /// Indicates the transmission of a segment of bundle data.
    XferSegment = 0x01,
    /// Acknowledges reception of a data segment.
    XferAck = 0x02,
    /// Indicates that the transmission of the current bundle SHALL be stopped.
    XferRefuse = 0x03,
    /// Used to keep TCPCL session active.
    Keepalive = 0x04,
    /// Indicates that one of the entities participating in the session wishes to cleanly terminate the session.
    SessTerm = 0x05,
    /// Contains a TCPCL message rejection.
    MsgReject = 0x06,
    /// Contains the session parameter inputs from one of the entities.
    SessInit = 0x07,
}

bitflags! {
    /// Session Extension Item flags
    pub(crate) struct SessionExtensionItemFlags : u8 {
        const CRITICAL = 0x01;
    }
}

/// MSG_REJECT Reason Codes
#[derive(FromPrimitive, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum MsgRejectReasonCode {
    /// A message was received with a Message Type code unknown to the TCPCL node.
    TypeUnknown = 0x01,
    /// A message was received but the TCPCL entity cannot comply with the message contents.
    Unsupported = 0x02,
    /// A message was received while the session is in a state in which the message is not expected.
    Unexpected = 0x03,
}

impl std::fmt::Debug for MsgRejectReasonCode {
    fn fmt(&self, f: &mut _core::fmt::Formatter<'_>) -> _core::fmt::Result {
        match self {
            Self::TypeUnknown => write!(f, "TypeUnknown"),
            Self::Unsupported => write!(f, "Unsupported"),
            Self::Unexpected => write!(f, "Unexpected"),
        }
    }
}

bitflags! {
    /// XFER_SEGMENT flags
    pub(crate) struct XferSegmentFlags : u8 {
        const END = 0x01;
        const START = 0x02;
    }
}

/// XFER_REFUSE Reason Codes
#[derive(Debug, FromPrimitive, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum XferRefuseReasonCode {
    /// Reason for refusal is unknown or not specified.
    Unknown = 0,
    /// The receiver already has the complete bundle. The sender MAY consider the bundle as completely received.
    Completed = 0x01,
    /// The receiver's resources are exhausted. The sender SHOULD apply reactive bundle fragmentation before retrying.
    NoResources = 0x02,
    /// The receiver has encountered a problem that requires the bundle to be retransmitted in its entirety.
    Retransmit = 0x03,
    /// Some issue with the bundle data or the transfer extension data was encountered. The sender SHOULD NOT retry the same bundle with the same extensions.
    NotAcceptable = 0x04,
    /// A failure processing the Transfer Extension Items has occurred.
    ExtensionFailure = 0x05,
}

bitflags! {
    /// Transfer Extension Item flags
    pub(crate) struct TransferExtensionItemFlags : u8 {
        const CRITICAL = 0x01;
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct TransferExtensionItem {
    pub flags: TransferExtensionItemFlags,
    pub item_type: TransferExtensionItemType,
    pub data: Bytes,
}

#[derive(Debug, PartialEq, Eq, FromPrimitive, Clone, Copy)]
#[repr(u16)]
pub(crate) enum TransferExtensionItemType {
    /// Sends the bundle id as extension, can be used to prevent retransmission of already existing bundles
    BundleID = 0x01,
}

bitflags! {
    /// SESS_TERM flags
    pub(crate) struct SessTermFlags : u8 {
        /// If bit is set, indicates that this message is an acknowledgement of an earlier SESS_TERM message.
        const REPLY = 0x01;
    }
}

/// SESS_TERM Reason Codes
#[derive(Debug, FromPrimitive, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SessTermReasonCode {
    /// A termination reason is not available.
    Unknown = 0,
    /// The session is being closed due to idleness.
    IdleTimeout = 0x01,
    /// The implemented version of the protocol is not supported.
    VersionMismatch = 0x02,
    /// Daemon is busy.
    Busy = 0x03,
    /// Peer was unable to be contacted.
    ContactFailure = 0x04,
    /// No resources available.
    ResourceExhaustion = 0x05,
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SessInitData {
    pub keepalive: u16,
    pub segment_mru: u64,
    pub transfer_mru: u64,
    pub node_id: String,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct XferSegData {
    pub flags: XferSegmentFlags,
    pub tid: u64,
    pub len: u64,
    pub buf: Bytes,
    pub extensions: Vec<TransferExtensionItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct XferAckData {
    pub flags: XferSegmentFlags,
    pub tid: u64,
    pub len: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct XferRefuseData {
    pub reason: XferRefuseReasonCode,
    pub tid: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessTermData {
    pub flags: SessTermFlags,
    pub reason: SessTermReasonCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MsgRejectData {
    pub reason: MsgRejectReasonCode,
    pub header: u8,
}
