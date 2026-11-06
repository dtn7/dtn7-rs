use std::io::Cursor;

use super::proto::*;
use log::{debug, warn};
use num_traits::FromPrimitive;
use thiserror::Error;
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MINIMUM_EXTENSION_ITEM_SIZE: u32 = 5;

#[derive(Debug, Eq, PartialEq, Clone)]
pub(crate) enum TcpClPacket {
    ContactHeader(ContactHeaderFlags),
    SessInit(SessInitData),
    SessTerm(SessTermData),
    XferSeg(XferSegData),
    XferAck(XferAckData),
    XferRefuse(XferRefuseData),
    KeepAlive,
    MsgReject(MsgRejectData),
}

impl TcpClPacket {
    pub async fn write(&self, writer: &mut (impl AsyncWrite + Unpin)) -> anyhow::Result<()> {
        match self {
            TcpClPacket::SessInit(sess_init_data) => {
                writer.write_u8(MessageType::SessInit as u8).await?;
                writer.write_u16(sess_init_data.keepalive).await?;
                writer.write_u64(sess_init_data.segment_mru).await?;
                writer.write_u64(sess_init_data.transfer_mru).await?;
                writer
                    .write_u16(sess_init_data.node_id.len() as u16)
                    .await?;
                let node_id_bytes = sess_init_data.node_id.as_bytes();
                writer.write_all(node_id_bytes).await?;
                writer.write_u32(0).await?;
            }
            TcpClPacket::SessTerm(sess_term_data) => {
                writer.write_u8(MessageType::SessTerm as u8).await?;
                writer.write_u8(sess_term_data.flags.bits()).await?;
                writer.write_u8(sess_term_data.reason as u8).await?;
            }
            TcpClPacket::XferSeg(xfer_seg_data) => {
                writer.write_u8(MessageType::XferSegment as u8).await?;
                writer.write_u8(xfer_seg_data.flags.bits()).await?;
                writer.write_u64(xfer_seg_data.tid).await?;
                if xfer_seg_data.flags.contains(XferSegmentFlags::START) {
                    let mut ext_data = Cursor::new(Vec::new());
                    let mut len = 0u32;
                    for ext in &xfer_seg_data.extensions {
                        ext_data.write_u8(ext.flags.bits()).await?;
                        ext_data.write_u16(ext.item_type as u16).await?;
                        ext_data.write_u16(ext.data.len() as u16).await?;
                        ext_data.write_all(ext.data.as_ref()).await?;
                        len += MINIMUM_EXTENSION_ITEM_SIZE + ext.data.len() as u32;
                    }
                    writer.write_u32(len).await?;
                    if len > 0 {
                        writer.write_all(ext_data.get_ref()).await?;
                    }
                }
                writer.write_u64(xfer_seg_data.len).await?;

                if xfer_seg_data.len > 0 {
                    writer.write_all(xfer_seg_data.buf.as_ref()).await?;
                }
            }
            TcpClPacket::XferAck(xfer_ack_data) => {
                writer.write_u8(MessageType::XferAck as u8).await?;
                writer.write_u8(xfer_ack_data.flags.bits()).await?;
                writer.write_u64(xfer_ack_data.tid).await?;
                writer.write_u64(xfer_ack_data.len).await?;
            }
            TcpClPacket::XferRefuse(xfer_refuse_data) => {
                writer.write_u8(MessageType::XferRefuse as u8).await?;
                writer.write_u8(xfer_refuse_data.reason as u8).await?;
                writer.write_u64(xfer_refuse_data.tid).await?;
            }
            TcpClPacket::KeepAlive => {
                writer.write_u8(MessageType::Keepalive as u8).await?;
            }
            TcpClPacket::MsgReject(msg_reject_data) => {
                writer.write_u8(MessageType::MsgReject as u8).await?;
                writer.write_u8(msg_reject_data.reason as u8).await?;
                writer.write_u8(msg_reject_data.header).await?;
            }
            TcpClPacket::ContactHeader(flags) => {
                writer.write_all(b"dtn!").await?;
                writer.write_u8(4).await?;
                writer.write_u8(flags.bits()).await?;
            }
        }
        writer.flush().await?;
        Ok(())
    }

    pub async fn read(reader: &mut (impl AsyncRead + Unpin)) -> Result<Self, TcpClError> {
        let mtype = reader.read_u8().await?;
        if let Some(mtype) = MessageType::from_u8(mtype) {
            match mtype {
                MessageType::XferSegment => {
                    let flags = XferSegmentFlags::from_bits_truncate(reader.read_u8().await?);
                    let tid: u64 = reader.read_u64().await?;
                    let mut extensions = Vec::new();
                    if flags.contains(XferSegmentFlags::START) {
                        let mut ext_len: u32 = reader.read_u32().await?;
                        // parse bundle ids that are request
                        if ext_len != 0 {
                            debug!("parsing transfer extensions");
                            while ext_len >= MINIMUM_EXTENSION_ITEM_SIZE {
                                let flag = reader.read_u8().await?;
                                let item_type = reader.read_u16().await?;
                                let item_length = reader.read_u16().await?;
                                ext_len = ext_len
                                    .checked_sub(MINIMUM_EXTENSION_ITEM_SIZE + item_length as u32)
                                    .ok_or(TcpClError::MalformedTransferExtension)?;
                                let mut data = vec![0; item_length as usize];
                                reader.read_exact(&mut data).await?;
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
                                    reader.read_u8().await?;
                                }
                            }
                        }
                    }
                    let len = reader.read_u64().await?;
                    debug!("Reading xfer segment with len {}", len);
                    // never allocate more than our advertised segment MRU
                    if len > super::SEGMENT_MRU {
                        return Err(TcpClError::SegmentTooLarge(len));
                    }
                    let mut data = vec![0; len as usize];
                    if len > 0 {
                        reader.read_exact(&mut data).await?;
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
                    let flags = XferSegmentFlags::from_bits_truncate(reader.read_u8().await?);
                    let tid: u64 = reader.read_u64().await?;
                    let len: u64 = reader.read_u64().await?;
                    let data = XferAckData { flags, tid, len };
                    Ok(TcpClPacket::XferAck(data))
                }
                MessageType::XferRefuse => {
                    let rcode = reader.read_u8().await?;
                    if let Some(reason) = XferRefuseReasonCode::from_u8(rcode) {
                        let tid: u64 = reader.read_u64().await?;
                        let data = XferRefuseData { reason, tid };
                        Ok(TcpClPacket::XferRefuse(data))
                    } else {
                        Err(TcpClError::UnknownReasonCode(rcode))
                    }
                }
                MessageType::Keepalive => Ok(TcpClPacket::KeepAlive),
                MessageType::SessTerm => {
                    let flags = SessTermFlags::from_bits_truncate(reader.read_u8().await?);
                    let rcode = reader.read_u8().await?;
                    if let Some(reason) = SessTermReasonCode::from_u8(rcode) {
                        let data = SessTermData { flags, reason };
                        Ok(TcpClPacket::SessTerm(data))
                    } else {
                        Err(TcpClError::UnknownReasonCode(rcode))
                    }
                }
                MessageType::SessInit => {
                    let keepalive = reader.read_u16().await?;
                    let segment_mru = reader.read_u64().await?;
                    let transfer_mru = reader.read_u64().await?;
                    let node_id_len = reader.read_u16().await? as usize;
                    let mut node_buffer = vec![0u8; node_id_len];
                    reader.read_exact(&mut node_buffer).await?;
                    let node_id: String = String::from_utf8_lossy(&node_buffer).into();
                    let ext_items = reader.read_u32().await?;

                    if ext_items != 0 {
                        return Err(TcpClError::SessionExtensionItemsUnsupported);
                    }
                    let data = SessInitData {
                        keepalive,
                        segment_mru,
                        transfer_mru,
                        node_id,
                    };
                    Ok(TcpClPacket::SessInit(data))
                }
                MessageType::MsgReject => {
                    let reason_code = reader.read_u8().await?;
                    let header = reader.read_u8().await?;
                    if let Some(reason) = MsgRejectReasonCode::from_u8(reason_code) {
                        let data = MsgRejectData { reason, header };
                        Ok(TcpClPacket::MsgReject(data))
                    } else {
                        Err(TcpClError::UnknownReasonCode(reason_code))
                    }
                }
            }
        } else if mtype == b'd' {
            // the first byte of the magic "dtn!" was already read as message type
            let mut buf: [u8; 5] = [0; 5];
            reader.read_exact(&mut buf).await?;
            if &buf[0..3] != b"tn!" {
                return Err(TcpClError::InvalidMagic);
            }
            if buf[3] != 4 {
                return Err(TcpClError::UnsupportedVersion);
            }
            Ok(TcpClPacket::ContactHeader(
                ContactHeaderFlags::from_bits_truncate(buf[4]),
            ))
        } else {
            // unknown  code
            Err(TcpClError::UnknownPacketType(mtype))
        }
    }
}

#[derive(Error, Debug)]
pub(crate) enum TcpClError {
    #[error("error reading bytes")]
    ReadError(#[from] io::Error),
    #[error("unknown packet type ({0}) encountered")]
    UnknownPacketType(u8),
    #[error("unknown reason code ({0}) encountered")]
    UnknownReasonCode(u8),
    #[error("session extension items found but unsupported")]
    SessionExtensionItemsUnsupported,
    #[error("unexpected packet received")]
    UnexpectedPacket,
    #[error("invalid magic in contact header")]
    InvalidMagic,
    #[error("unsupported version in contact header")]
    UnsupportedVersion,
    #[error("transfer extension items exceed their announced length")]
    MalformedTransferExtension,
    #[error("transfer segment of {0} bytes exceeds segment MRU")]
    SegmentTooLarge(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn contact_header_roundtrip() {
        let packet = TcpClPacket::ContactHeader(ContactHeaderFlags::default());
        let mut buf = Vec::new();
        packet.write(&mut buf).await.unwrap();
        let mut slice = buf.as_slice();
        assert_eq!(TcpClPacket::read(&mut slice).await.unwrap(), packet);
        assert!(slice.is_empty(), "contact header not fully consumed");
    }

    #[tokio::test]
    async fn oversized_segment_is_rejected_without_allocating() {
        let mut buf = vec![MessageType::XferSegment as u8, XferSegmentFlags::END.bits()];
        buf.extend_from_slice(&1u64.to_be_bytes()); // tid
        buf.extend_from_slice(&u64::MAX.to_be_bytes()); // len
        let mut slice = buf.as_slice();
        match TcpClPacket::read(&mut slice).await {
            Err(TcpClError::SegmentTooLarge(len)) => assert_eq!(len, u64::MAX),
            other => panic!("expected SegmentTooLarge, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn segment_at_mru_is_accepted() {
        let packet = TcpClPacket::XferSeg(XferSegData {
            flags: XferSegmentFlags::START | XferSegmentFlags::END,
            tid: 1,
            len: super::super::SEGMENT_MRU,
            buf: vec![0x42; super::super::SEGMENT_MRU as usize].into(),
            extensions: Vec::new(),
        });
        let mut buf = Vec::new();
        packet.write(&mut buf).await.unwrap();
        let mut slice = buf.as_slice();
        assert_eq!(TcpClPacket::read(&mut slice).await.unwrap(), packet);
    }

    #[tokio::test]
    async fn transfer_extension_exceeding_announced_length_is_rejected() {
        let mut buf = vec![
            MessageType::XferSegment as u8,
            (XferSegmentFlags::START | XferSegmentFlags::END).bits(),
        ];
        buf.extend_from_slice(&1u64.to_be_bytes()); // tid
        buf.extend_from_slice(&MINIMUM_EXTENSION_ITEM_SIZE.to_be_bytes()); // ext_len
        buf.push(0); // item flags
        buf.extend_from_slice(&1u16.to_be_bytes()); // item type
        buf.extend_from_slice(&100u16.to_be_bytes()); // item length > ext_len
        buf.extend_from_slice(&[0u8; 100]);
        let mut slice = buf.as_slice();
        match TcpClPacket::read(&mut slice).await {
            Err(TcpClError::MalformedTransferExtension) => {}
            other => panic!("expected MalformedTransferExtension, got {:?}", other),
        }
    }
}
