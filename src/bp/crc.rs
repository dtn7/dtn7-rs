use super::bundle::*;

/******************************
 *
 * CRC
 *
 ******************************/

pub type CRCType = u8;

use byteorder::{BigEndian, ByteOrder};
use crc::{crc16, crc32};

pub const CRC_NO: CRCType = 0;
pub const CRC_16: CRCType = 1;
pub const CRC_32: CRCType = 2;

pub trait CRCFuncations {
    fn to_string(self) -> String;
}
impl CRCFuncations for CRCType {
    fn to_string(self) -> String {
        match self {
            CRC_NO => String::from("no"),
            CRC_16 => String::from("16"),
            CRC_32 => String::from("32"),
            _ => String::from("unknown"),
        }
    }
}
pub fn empty_crc(crc_type: CRCType) -> Result<ByteBuffer, Bp7Error> {
    match crc_type {
        CRC_NO => Ok(Vec::new()),
        CRC_16 => Ok(vec![0; 2]),
        CRC_32 => Ok(vec![0; 4]),
        _ => Err(Bp7Error::CrcError("Unknown CRC type".to_string())),
    }
}

pub fn block_to_bytes<T: Block + Clone>(blck: &T) -> ByteBuffer {
    let temp_blck = &mut blck.clone();
    temp_blck.reset_crc();
    temp_blck.to_cbor()
}
pub fn calculate_crc<T: Block + Clone>(blck: &T) -> ByteBuffer {
    let mut output_crc = empty_crc(blck.crc_type()).unwrap();
    let data = block_to_bytes(blck);
    match blck.crc_type() {
        CRC_NO => return output_crc,
        CRC_16 => {
            let chksm = crc16::checksum_x25(&data);
            BigEndian::write_u16(&mut output_crc, chksm);
        }
        CRC_32 => {
            let chksm = crc32::checksum_castagnoli(&data);
            BigEndian::write_u32(&mut output_crc, chksm);
        }
        _ => {
            panic!("Unknown crc type");
        }
    }

    output_crc
}
pub fn check_crc<T: Block + Clone>(blck: &T) -> bool {
    if !blck.has_crc() {
        return blck.has_crc();
    }
    (&blck).crc() == calculate_crc(blck)
}
