use std::io::{Cursor, Error, ErrorKind, Read};

use super::{proto::*, SEGMENT_MRU};
use byteorder::{BigEndian, ReadBytesExt};
use bytes::{Buf, BufMut, BytesMut};
use log::{error, trace, warn};
use num_traits::FromPrimitive;
use thiserror::Error;
use tokio::{
    io::{self},
    time::Instant,
};
use tokio_util::codec::{Decoder, Encoder};

const MINIMUM_EXTENSION_ITEM_SIZE: u32 = 5;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum TcpClPacket {
    ContactHeader(ContactHeaderFlags),
    SessInit(SessInitData),
    SessTerm(SessTermData),
    XferSeg(XferSegData),
    XferAck(XferAckData),
    XferRefuse(XferRefuseData),
    KeepAlive,
    MsgReject(MsgRejectData),
    BundleIDRequest(BundleIDRequestData),
    BundleIDResponse(BundleIDResponseData),
}

pub struct TcpClCodec {
    pub startup: bool,
}

impl Encoder<TcpClPacket> for TcpClCodec {
    type Error = io::Error;

    fn encode(&mut self, item: TcpClPacket, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let now = Instant::now();
        if dst.len() < SEGMENT_MRU as usize {
            dst.reserve(SEGMENT_MRU as usize - dst.len());
        }
        item.write(dst);
        trace!("Time encode {:?}, {:?}", now.elapsed(), item);
        Ok(())
    }
}

impl Decoder for TcpClCodec {
    type Item = TcpClPacket;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<TcpClPacket>> {
        let now = Instant::now();
        if buf.is_empty() {
            return Ok(None);
        }

        if buf.len() > SEGMENT_MRU as usize {
            return Err(Error::new(
                ErrorKind::OutOfMemory,
                "Buffer greater than maximum segment size",
            ));
        }
        let len = buf.len();
        let mut cursor = Cursor::new(buf);

        match TcpClPacket::read(&mut cursor) {
            Ok(packet) => {
                if matches!(packet, TcpClPacket::ContactHeader(_)) {
                    if self.startup {
                        self.startup = false;
                    } else {
                        return Ok(None);
                    }
                }
                let pos = cursor.position() as usize;
                cursor.into_inner().advance(pos);
                trace!("Time decode {:?}, {:?}", now.elapsed(), packet);
                Ok(Some(packet))
            }
            Err(err) => {
                trace!("error while parsing: {}", err);
                cursor.into_inner().reserve(SEGMENT_MRU as usize - len);
                Ok(None)
            }
        }
    }
}

impl TcpClPacket {
    pub fn write(&self, writer: &mut impl BufMut) {
        match self {
            TcpClPacket::SessInit(sess_init_data) => {
                writer.put_u8(MessageType::SessInit as u8);
                writer.put_u16(sess_init_data.keepalive);
                writer.put_u64(sess_init_data.segment_mru);
                writer.put_u64(sess_init_data.transfer_mru);
                writer.put_u16(sess_init_data.node_id.len() as u16);
                let node_id_bytes = sess_init_data.node_id.as_bytes();
                writer.put_slice(node_id_bytes);
                let mut ext_data = BytesMut::new();
                let mut len = 0u32;
                for ext in &sess_init_data.extensions {
                    ext_data.put_u8(ext.flags.bits());
                    ext_data.put_u16(ext.item_type as u16);
                    ext_data.put_u16(ext.data.len() as u16);
                    ext_data.put_slice(ext.data.as_ref());
                    len += MINIMUM_EXTENSION_ITEM_SIZE + ext.data.len() as u32;
                }
                writer.put_u32(len);
                if len > 0 {
                    writer.put_slice(ext_data.as_ref());
                }
            }
            TcpClPacket::SessTerm(sess_term_data) => {
                writer.put_u8(MessageType::SessTerm as u8);
                writer.put_u8(sess_term_data.flags.bits());
                writer.put_u8(sess_term_data.reason as u8);
            }
            TcpClPacket::XferSeg(xfer_seg_data) => {
                writer.put_u8(MessageType::XferSegment as u8);
                writer.put_u8(xfer_seg_data.flags.bits());
                writer.put_u64(xfer_seg_data.tid);
                if xfer_seg_data.flags.contains(XferSegmentFlags::START) {
                    let mut ext_data = BytesMut::new();
                    let mut len = 0u32;
                    for ext in &xfer_seg_data.extensions {
                        ext_data.put_u8(ext.flags.bits());
                        ext_data.put_u16(ext.item_type as u16);
                        ext_data.put_u16(ext.data.len() as u16);
                        ext_data.put_slice(ext.data.as_ref());
                        len += MINIMUM_EXTENSION_ITEM_SIZE + ext.data.len() as u32;
                    }
                    writer.put_u32(len);
                    if len > 0 {
                        writer.put_slice(ext_data.as_ref());
                    }
                }
                writer.put_u64(xfer_seg_data.len);

                if xfer_seg_data.len > 0 {
                    writer.put_slice(xfer_seg_data.buf.as_ref());
                }
            }
            TcpClPacket::XferAck(xfer_ack_data) => {
                writer.put_u8(MessageType::XferAck as u8);
                writer.put_u8(xfer_ack_data.flags.bits());
                writer.put_u64(xfer_ack_data.tid);
                writer.put_u64(xfer_ack_data.len);
            }
            TcpClPacket::XferRefuse(xfer_refuse_data) => {
                writer.put_u8(MessageType::XferRefuse as u8);
                writer.put_u8(xfer_refuse_data.reason as u8);
                writer.put_u64(xfer_refuse_data.tid);
            }
            TcpClPacket::KeepAlive => {
                writer.put_u8(MessageType::Keepalive as u8);
            }
            TcpClPacket::MsgReject(msg_reject_data) => {
                writer.put_u8(MessageType::MsgReject as u8);
                writer.put_u8(msg_reject_data.reason as u8);
                writer.put_u8(msg_reject_data.header);
            }
            TcpClPacket::ContactHeader(flags) => {
                writer.put_slice(b"dtn!");
                writer.put_u8(4);
                writer.put_u8(flags.bits());
            }
            TcpClPacket::BundleIDRequest(data) => {
                writer.put_u8(MessageType::BundleIDRequest as u8);
                writer.put_u64(data.tid);
                writer.put_u16(data.data.len() as u16);
                writer.put_slice(data.data.as_ref());
            }
            TcpClPacket::BundleIDResponse(data) => {
                writer.put_u8(MessageType::BundleIDResponse as u8);
                writer.put_u64(data.tid);
                writer.put_u8(data.code as u8);
            }
        }
    }

    pub fn read(reader: &mut impl Read) -> anyhow::Result<Self> {
        let mtype = reader.read_u8()?;
        if let Some(mtype) = MessageType::from_u8(mtype) {
            match mtype {
                MessageType::XferSegment => {
                    let flags = XferSegmentFlags::from_bits_truncate(reader.read_u8()?);
                    let tid: u64 = reader.read_u64::<BigEndian>()?;
                    let mut extensions = Vec::new();
                    if flags.contains(XferSegmentFlags::START) {
                        let mut ext_len: u32 = reader.read_u32::<BigEndian>()?;
                        // parse bundle ids that are request
                        if ext_len != 0 {
                            while ext_len >= MINIMUM_EXTENSION_ITEM_SIZE {
                                let flag = reader.read_u8()?;
                                let item_type = reader.read_u16::<BigEndian>()?;
                                let item_length = reader.read_u16::<BigEndian>()?;
                                ext_len =
                                    ext_len - MINIMUM_EXTENSION_ITEM_SIZE - item_length as u32;
                                let mut data = vec![0; item_length as usize];
                                reader.read_exact(&mut data)?;
                                if let Some(item_type) =
                                    TransferExtensionItemType::from_u16(item_type)
                                {
                                    let transfer_extension = TransferExtensionItem {
                                        flags: TransferExtensionItemFlags::from_bits_truncate(flag),
                                        item_type,
                                        data: data.into(),
                                    };
                                    extensions.push(transfer_extension);
                                }
                            }
                            if ext_len != 0 {
                                warn!("malformed transfer extensions, ignoring rest");
                                for _ in 0..ext_len {
                                    reader.read_u8()?;
                                }
                            }
                        }
                    }
                    let len = reader.read_u64::<BigEndian>()?;
                    let mut data = vec![0; len as usize];
                    if len > 0 {
                        reader.read_exact(&mut data)?;
                    }

                    let seg = XferSegData {
                        flags,
                        tid,
                        len,
                        buf: data.into(),
                        extensions,
                    };

                    Ok(TcpClPacket::XferSeg(seg))
                }
                MessageType::XferAck => {
                    let flags = XferSegmentFlags::from_bits_truncate(reader.read_u8()?);
                    let tid: u64 = reader.read_u64::<BigEndian>()?;
                    let len: u64 = reader.read_u64::<BigEndian>()?;
                    let data = XferAckData { flags, tid, len };
                    Ok(TcpClPacket::XferAck(data))
                }
                MessageType::XferRefuse => {
                    let rcode = reader.read_u8()?;
                    if let Some(reason) = XferRefuseReasonCode::from_u8(rcode) {
                        let tid: u64 = reader.read_u64::<BigEndian>()?;
                        let data = XferRefuseData { reason, tid };
                        Ok(TcpClPacket::XferRefuse(data))
                    } else {
                        Err(TcpClError::UnknownResaonCode(rcode).into())
                    }
                }
                MessageType::Keepalive => Ok(TcpClPacket::KeepAlive),
                MessageType::SessTerm => {
                    let flags = SessTermFlags::from_bits_truncate(reader.read_u8()?);
                    let rcode = reader.read_u8()?;
                    if let Some(reason) = SessTermReasonCode::from_u8(rcode) {
                        let data = SessTermData { flags, reason };
                        Ok(TcpClPacket::SessTerm(data))
                    } else {
                        Err(TcpClError::UnknownResaonCode(rcode).into())
                    }
                }
                MessageType::SessInit => {
                    let keepalive = reader.read_u16::<BigEndian>()?;
                    let segment_mru = reader.read_u64::<BigEndian>()?;
                    let transfer_mru = reader.read_u64::<BigEndian>()?;
                    let node_id_len = reader.read_u16::<BigEndian>()? as usize;
                    let mut node_buffer = vec![0u8; node_id_len];
                    reader.read_exact(&mut node_buffer)?;
                    let node_id: String = String::from_utf8_lossy(&node_buffer).into();
                    let mut ext_len: u32 = reader.read_u32::<BigEndian>()?;
                    let mut extensions = Vec::new();
                    if ext_len != 0 {
                        while ext_len >= MINIMUM_EXTENSION_ITEM_SIZE {
                            let flag = reader.read_u8()?;
                            let item_type = reader.read_u16::<BigEndian>()?;
                            let item_length = reader.read_u16::<BigEndian>()?;
                            ext_len = ext_len - MINIMUM_EXTENSION_ITEM_SIZE - item_length as u32;
                            let mut data = vec![0; item_length as usize];
                            reader.read_exact(&mut data)?;
                            if let Some(item_type) = SessionExtensionItemType::from_u16(item_type) {
                                let transfer_extension = SessionExtensionItem {
                                    flags: SessionExtensionItemFlags::from_bits_truncate(flag),
                                    item_type,
                                    data: data.into(),
                                };
                                extensions.push(transfer_extension);
                            }
                        }
                        if ext_len != 0 {
                            return Err(TcpClError::MalformedPacket(
                                "session extension, unread bytes left",
                            )
                            .into());
                        }
                    }
                    let data = SessInitData {
                        keepalive,
                        segment_mru,
                        transfer_mru,
                        node_id,
                        extensions,
                    };
                    Ok(TcpClPacket::SessInit(data))
                }
                MessageType::MsgReject => {
                    let reason_code = reader.read_u8()?;
                    let header = reader.read_u8()?;
                    if let Some(reason) = MsgRejectReasonCode::from_u8(reason_code) {
                        let data = MsgRejectData { reason, header };
                        Ok(TcpClPacket::MsgReject(data))
                    } else {
                        Err(TcpClError::UnknownResaonCode(reason_code).into())
                    }
                }
                MessageType::BundleIDRequest => {
                    let tid = reader.read_u64::<BigEndian>()?;
                    let data_len = reader.read_u16::<BigEndian>()?;
                    let mut data = vec![0; data_len as usize];
                    reader.read_exact(&mut data)?;
                    Ok(TcpClPacket::BundleIDRequest(BundleIDRequestData {
                        tid,
                        data: data.into(),
                    }))
                }
                MessageType::BundleIDResponse => {
                    let tid = reader.read_u64::<BigEndian>()?;
                    if let Some(code) = BundleIDResponse::from_u8(reader.read_u8()?) {
                        Ok(TcpClPacket::BundleIDResponse(BundleIDResponseData {
                            tid,
                            code,
                        }))
                    } else {
                        Err(TcpClError::MalformedPacket("unsupported bundle id response").into())
                    }
                }
            }
        } else if mtype == b'd' {
            let mut buf: [u8; 5] = [0; 5];
            reader.read_exact(&mut buf)?;
            if &buf[0..3] != b"tn!" {
                return Err(TcpClError::InvalidMagic.into());
            }
            if buf[3] != 4 {
                return Err(TcpClError::UnsupportedVersion.into());
            }
            Ok(TcpClPacket::ContactHeader(
                ContactHeaderFlags::from_bits_truncate(buf[4]),
            ))
        } else {
            // unknown  code
            Err(TcpClError::UnknownPacketType(mtype).into())
        }
    }
}

#[derive(Error, Debug)]
pub enum TcpClError {
    #[error("error reading bytes")]
    ReadError(#[from] io::Error),
    #[error("unknown packet type ({0}) encountered")]
    UnknownPacketType(u8),
    #[error("unknown reason code ({0}) encountered")]
    UnknownResaonCode(u8),
    #[error("session extension items found but unsupported")]
    SessionExtensionItemsUnsupported,
    #[error("unexpected packet received")]
    UnexpectedPacket,
    #[error("invalid magic in contact header")]
    InvalidMagic,
    #[error("unsupported version in contact header")]
    UnsupportedVersion,
    #[error("malformed packet: ({0})")]
    MalformedPacket(&'static str),
}
