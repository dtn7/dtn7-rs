use super::proto::*;
use log::{debug, error, info};
//use std::net::TcpStream;
use anyhow::bail;
use bytes::Bytes;
use num_traits::FromPrimitive;
use std::{
    io::Cursor,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};
use thiserror::Error;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
#[derive(Debug)]
pub(crate) enum TcpClPacket {
    SessInit(SessInitData),
    SessTerm(SessTermData),
    XferSeg,
    XferAck(XferAckData),
    XferRefuse(XferRefuseData),
    KeepAlive,
    MsgReject,
}

#[derive(Error, Debug)]
pub(crate) enum TcpClError {
    #[error("not enough bytes in buffer to parse packet from")]
    NotEnoughBytesReceived,
    #[error("error reading bytes")]
    ReadError(#[from] io::Error),
    #[error("unknown packet type ({0}) encountered")]
    UnknownPacketType(u8),
    #[error("unknown reason code ({0}) encountered")]
    UnknownResaonCode(u8),
    #[error("session extension items found but unsupported")]
    SessionExtensionItemsUnsupported,
}

pub(crate) async fn parses_packet(buffer: &mut bytes::BytesMut) -> Result<TcpClPacket, TcpClError> {
    let mut buf = Cursor::new(&buffer[..]);

    let mtype = buf.read_u8().await?;
    if let Some(mtype) = MessageType::from_u8(mtype) {
        match mtype {
            MessageType::XFER_SEGMENT => {
                // TODO
                Ok(TcpClPacket::XferSeg)
            }
            MessageType::XFER_ACK => {
                if buffer.len() < 18 {
                    return Err(TcpClError::NotEnoughBytesReceived);
                }
                /*let flags = XferSegmentFlags::from_bits_truncate(buf[1]);
                let tid : u64 = u64::from_be_bytes(buf[2..10].try_into()?);
                let len : u64 = u64::from_be_bytes(buf[10..18].try_into()?);*/
                let flags = XferSegmentFlags::from_bits_truncate(buf.read_u8().await?);
                let tid: u64 = buf.read_u64().await?;
                let len: u64 = buf.read_u64().await?;
                let data = XferAckData { flags, tid, len };

                let pkt_len = buf.position() as usize;
                let _ = buffer.split_to(pkt_len);
                Ok(TcpClPacket::XferAck(data))
            }
            MessageType::XFER_REFUSE => {
                if buffer.len() < 10 {
                    return Err(TcpClError::NotEnoughBytesReceived);
                }
                let rcode = buf.read_u8().await?;
                if let Some(reason) = XferRefuseReasonCode::from_u8(rcode) {
                    let tid: u64 = buf.read_u64().await?;
                    let data = XferRefuseData { reason, tid };

                    let pkt_len = buf.position() as usize;
                    let _ = buffer.split_to(pkt_len);
                    Ok(TcpClPacket::XferRefuse(data))
                } else {
                    return Err(TcpClError::UnknownResaonCode(rcode));
                }
            }
            MessageType::KEEPALIVE => {
                let pkt_len = buf.position() as usize;
                let _ = buffer.split_to(pkt_len);

                Ok(TcpClPacket::KeepAlive)
            }
            MessageType::SESS_TERM => {
                if buffer.len() < 3 {
                    return Err(TcpClError::NotEnoughBytesReceived);
                }
                let flags = SessTermFlags::from_bits_truncate(buf.read_u8().await?);
                let rcode = buf.read_u8().await?;
                if let Some(reason) = SessTermReasonCode::from_u8(rcode) {
                    let data = SessTermData { flags, reason };

                    let pkt_len = buf.position() as usize;
                    let _ = buffer.split_to(pkt_len);
                    Ok(TcpClPacket::SessTerm(data))
                } else {
                    return Err(TcpClError::UnknownResaonCode(rcode));
                }
            }
            MessageType::SESS_INIT => {
                if buffer.len() < 20 {
                    return Err(TcpClError::NotEnoughBytesReceived);
                }
                let keepalive = buf.read_u16().await?;
                let segment_mru = buf.read_u64().await?;
                let transfer_mru = buf.read_u64().await?;
                let node_id_len = buf.read_u16().await? as usize;
                let mut node_buffer = vec![0u8; node_id_len];
                buf.read_exact(&mut node_buffer).await?;
                let node_id: String = String::from_utf8_lossy(&node_buffer).into();
                let ext_items = buf.read_u32().await?;

                let pkt_len = buf.position() as usize;
                let _ = buffer.split_to(pkt_len);

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
            MessageType::MSG_REJECT => {
                // TODO
                Ok(TcpClPacket::MsgReject)
            }
        }
    } else {
        // unknown  code
        return Err(TcpClError::UnknownPacketType(mtype));
    }
}
pub(crate) async fn send_keepalive(
    socket: &mut tokio::net::tcp::OwnedWriteHalf,
) -> anyhow::Result<()> {
    socket.write_u8(MessageType::KEEPALIVE as u8).await?;
    socket.flush().await?;
    Ok(())
}

pub(crate) async fn send_contact_header(socket: &mut TcpStream) -> anyhow::Result<()> {
    let ch_flags: ContactHeaderFlags = Default::default();
    socket.write(b"dtn!").await?;
    socket.write_u8(4).await?;
    socket.write_u8(ch_flags.bits()).await?;
    socket.flush().await?;
    Ok(())
}
pub(crate) async fn send_sess_term(
    socket: &mut TcpStream,
    reason: SessTermReasonCode,
    flags: SessTermFlags,
) -> anyhow::Result<()> {
    socket.write_u8(MessageType::SESS_TERM as u8).await?;
    socket.write_u8(flags.bits()).await?;
    socket.write_u8(reason as u8).await?;
    socket.flush().await?;
    Ok(())
}
pub(crate) async fn send_sess_init(
    socket: &mut TcpStream,
    data: SessInitData,
) -> anyhow::Result<()> {
    socket.write_u8(MessageType::SESS_INIT as u8).await?;
    socket.write_u16(data.keepalive).await?;
    socket.write_u64(data.segment_mru).await?;
    socket.write_u64(data.transfer_mru).await?;
    socket.write_u16(data.node_id.len() as u16).await?;
    socket.write_all(data.node_id.as_bytes()).await?;
    socket.write_u32(0).await?; // no extension items supported
    socket.flush().await?;
    Ok(())
}

pub(crate) async fn send_xfer_ack(
    socket: &mut tokio::net::tcp::OwnedWriteHalf,
    flags: XferSegmentFlags,
    transfer_id: u64,
    ack_len: u64,
) -> anyhow::Result<()> {
    socket.write_u8(MessageType::XFER_SEGMENT as u8).await?;
    socket.write_u8(flags.bits()).await?;
    socket.write_u64(transfer_id).await?;
    socket.write_u64(ack_len).await?;
    socket.flush().await?;
    Ok(())
}
pub(crate) async fn send_xfer_segment(
    socket: &mut tokio::net::tcp::OwnedWriteHalf,
    seg: XferSegData,
) -> anyhow::Result<()> {
    socket.write_u8(MessageType::XFER_ACK as u8).await?;
    socket.write_u8(seg.flags.bits()).await?;
    socket.write_u64(seg.tid).await?;
    socket.write_u32(0).await?;
    socket.write_u64(seg.len).await?;
    socket.write_all(&seg.buf).await?;
    socket.flush().await?;
    Ok(())
}
pub(crate) async fn send_xfer_refuse(
    socket: &mut tokio::net::tcp::OwnedWriteHalf,
    reason: XferRefuseReasonCode,
    transfer_id: u64,
) -> anyhow::Result<()> {
    socket.write_u8(MessageType::XFER_REFUSE as u8).await?;
    socket.write_u8(reason as u8).await?;
    socket.write_u64(transfer_id).await?;
    socket.flush().await?;
    Ok(())
}

pub(crate) fn generate_xfer_segments(
    config: &SessInitData,
    buf: Bytes,
) -> anyhow::Result<Vec<XferSegData>> {
    static LAST_TRANSFER_ID: AtomicU64 = AtomicU64::new(0);
    // TODO: check for wrap around and SESS_TERM if overflow occurs
    let tid = LAST_TRANSFER_ID.fetch_add(1, Ordering::SeqCst);
    let mut segs = Vec::new();

    if buf.len() > config.transfer_mru as usize {
        bail!("bundle too big");
    }
    let fitting = if buf.len() as u64 % config.segment_mru == 0 {
        0
    } else {
        1
    };
    let num_segs = (buf.len() as u64 / config.segment_mru) + fitting;

    for i in 0..num_segs {
        let mut flags = XferSegmentFlags::empty();
        if i == 0 {
            flags |= XferSegmentFlags::START;
        }
        if i == num_segs - 1 {
            flags |= XferSegmentFlags::END;
        }
        let len = if num_segs == 1 {
            // data fits in one segment
            buf.len() as u64
        } else if i == num_segs - 1 {
            // segment is the last one remaining
            buf.len() as u64 % config.segment_mru
        } else {
            // middle segment get filled to the max
            config.segment_mru
        };
        let base = (i * config.segment_mru) as usize;
        let seg = XferSegData {
            flags,
            tid,
            len,
            buf: buf.slice(base..base + len as usize),
        };
        segs.push(seg);
    }

    Ok(segs)
}

pub(crate) async fn receive_contact_header(
    socket: &mut TcpStream,
) -> anyhow::Result<ContactHeaderFlags> {
    let mut buf: [u8; 6] = [0; 6];
    //let ch_flags: ContactHeaderFlags = Default::default();
    socket.read_exact(&mut buf).await?;

    if &buf[0..4] != b"dtn!" {
        bail!("Invalid magic");
    }

    if buf[4] != 4 {
        bail!("Unsupported version");
    }

    Ok(ContactHeaderFlags::from_bits_truncate(buf[5]))
}
