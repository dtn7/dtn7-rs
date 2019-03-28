use super::bundle::*;
use super::crc::*;
use super::eid::*;
use derive_builder::Builder;
use serde::{Deserialize, Serialize};

/******************************
 *
 * Canonical Block
 *
 ******************************/

pub type CanonicalBlockType = u64;

// PAYLOAD_BLOCK is a BlockType for a payload block as defined in 4.2.3.
pub const PAYLOAD_BLOCK: CanonicalBlockType = 1;

// INTEGRITY_BLOCK is a BlockType defined in the Bundle Security Protocol
// specifiation.
pub const INTEGRITY_BLOCK: CanonicalBlockType = 2;

// CONFIDENTIALITY_BLOCK is a BlockType defined in the Bundle Security
// Protocol specifiation.
pub const CONFIDENTIALITY_BLOCK: CanonicalBlockType = 3;

// MANIFEST_BLOCK is a BlockType defined in the Manifest Extension Block
// specifiation.
pub const MANIFEST_BLOCK: CanonicalBlockType = 4;

// FLOW_LABEL_BLOCK is a BlockType defined in the Flow Label Extension Block
// specification.
pub const FLOW_LABEL_BLOCK: CanonicalBlockType = 6;

// PREVIOUS_NODE_BLOCK is a BlockType for a Previous Node block as defined
// in section 4.3.1.
pub const PREVIOUS_NODE_BLOCK: CanonicalBlockType = 7;

// BUNDLE_AGE_BLOCK is a BlockType for a Bundle Age block as defined in
// section 4.3.2.
pub const BUNDLE_AGE_BLOCK: CanonicalBlockType = 8;

// HOP_COUNT_BLOCK is a BlockType for a Hop Count block as defined in
// section 4.3.3.
pub const HOP_COUNT_BLOCK: CanonicalBlockType = 9;

//#[derive(Debug, Serialize_tuple, Deserialize_tuple, Clone)]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Builder)]
#[builder(default)]
pub struct CanonicalBlock {
    pub block_type: CanonicalBlockType,
    pub block_number: u64,
    pub block_control_flags: BlockControlFlags,
    pub crc_type: CRCType,
    data: CanonicalData,
    crc: ByteBuffer,
}

impl Default for CanonicalBlock {
    fn default() -> Self {
        CanonicalBlock::new()
    }
}
impl Block for CanonicalBlock {
    fn to_variant(&self) -> BlockVariants {
        if self.crc_type == CRC_NO {
            BlockVariants::CanonicalWithoutCrc(
                self.block_type,
                self.block_number,
                self.block_control_flags,
                self.crc_type,
                self.data.clone(),
            )
        } else {
            BlockVariants::Canonical(
                self.block_type,
                self.block_number,
                self.block_control_flags,
                self.crc_type,
                self.data.clone(),
                self.crc.clone(),
            )
        }
    }
    fn has_crc(&self) -> bool {
        self.crc_type != CRC_NO
    }
    fn crc(&self) -> ByteBuffer {
        self.crc.clone()
    }
    fn set_crc_type(&mut self, crc_type: CRCType) {
        self.crc_type = crc_type;
    }
    fn crc_type(&self) -> CRCType {
        self.crc_type
    }
    fn set_crc(&mut self, crc: ByteBuffer) {
        self.crc = crc;
    }
}

pub fn new_canonical_block(
    block_type: CanonicalBlockType,
    block_number: u64,
    block_control_flags: BlockControlFlags,
    data: CanonicalData,
) -> CanonicalBlock {
    CanonicalBlock {
        block_type,
        block_number,
        block_control_flags,
        crc_type: CRC_NO,
        data,
        crc: Vec::new(),
    }
}

impl CanonicalBlock {
    pub fn new() -> CanonicalBlock {
        CanonicalBlock {
            block_type: PAYLOAD_BLOCK,
            block_number: 0,
            block_control_flags: 0,
            crc_type: CRC_NO,
            data: CanonicalData::Data(Vec::new()),
            crc: Vec::new(),
        }
    }
    pub fn validation_errors(&self) -> Option<Bp7ErrorList> {
        let mut errors: Bp7ErrorList = Vec::new();

        if let Some(err) = self.block_control_flags.validation_error() {
            errors.push(err);
        }

        if let Some(err) = self.extension_validation_error() {
            errors.push(err);
        }

        if !errors.is_empty() {
            return Some(errors);
        }
        None
    }
    pub fn extension_validation_error(&self) -> Option<Bp7Error> {
        match &self.data {
            CanonicalData::Data(_) => {
                if self.block_type != PAYLOAD_BLOCK {
                    return Some(Bp7Error::CanonicalBlockError(
                        "Payload data not matching payload type".to_string(),
                    ));
                }
                if self.block_number != 0 {
                    return Some(Bp7Error::CanonicalBlockError(
                        "Payload Block's block number is not zero".to_string(),
                    ));
                }
            }
            CanonicalData::BundleAge(_) => {
                if self.block_type != BUNDLE_AGE_BLOCK {
                    return Some(Bp7Error::CanonicalBlockError(
                        "Payload data not matching payload type".to_string(),
                    ));
                }
            }
            CanonicalData::HopCount(_, _) => {
                if self.block_type != HOP_COUNT_BLOCK {
                    return Some(Bp7Error::CanonicalBlockError(
                        "Payload data not matching payload type".to_string(),
                    ));
                }
            }
            CanonicalData::PreviousNode(prev_eid) => {
                if self.block_type != PREVIOUS_NODE_BLOCK {
                    return Some(Bp7Error::CanonicalBlockError(
                        "Payload data not matching payload type".to_string(),
                    ));
                }
                if let Some(err) = prev_eid.validation_error() {
                    return Some(err);
                }
            }
        }
        if (self.block_type > 9 && self.block_type < 192) || (self.block_type > 255) {
            return Some(Bp7Error::CanonicalBlockError(
                "Unknown block type".to_string(),
            ));
        }

        None
    }
    pub fn get_data(&mut self) -> &CanonicalData {
        &self.data
    }
    pub fn set_data(&mut self, data: CanonicalData) {
        self.data = data;
    }
}

impl From<BlockVariants> for CanonicalBlock {
    fn from(item: BlockVariants) -> Self {
        match item {
            BlockVariants::CanonicalWithoutCrc(
                block_type,
                block_number,
                block_control_flags,
                crc_type,
                data,
            ) => CanonicalBlock {
                block_type,
                block_number,
                block_control_flags,
                crc_type,
                data: data.clone(),
                crc: Vec::new(),
            },
            BlockVariants::Canonical(
                block_type,
                block_number,
                block_control_flags,
                crc_type,
                data,
                crc,
            ) => CanonicalBlock {
                block_type,
                block_number,
                block_control_flags,
                crc_type,
                data: data.clone(),
                crc: crc.clone(),
            },
            _ => panic!("Error parsing canonical block"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum CanonicalData {
    HopCount(u32, u32),
    PreviousNode(EndpointID),
    BundleAge(u64),
    Data(#[serde(with = "serde_bytes")] ByteBuffer),
}

pub fn new_hop_count_block(
    block_number: u64,
    bcf: BlockControlFlags,
    limit: u32,
) -> CanonicalBlock {
    CanonicalBlockBuilder::default()
        .block_type(HOP_COUNT_BLOCK)
        .block_number(block_number)
        .block_control_flags(bcf)
        .data(CanonicalData::HopCount(0, limit))
        .build()
        .unwrap()
}

pub fn new_payload_block(bcf: BlockControlFlags, data: ByteBuffer) -> CanonicalBlock {
    CanonicalBlockBuilder::default()
        .block_type(PAYLOAD_BLOCK)
        .block_number(0)
        .block_control_flags(bcf)
        .data(CanonicalData::Data(data))
        .build()
        .unwrap()
}

pub fn new_previous_node_block(
    block_number: u64,
    bcf: BlockControlFlags,
    prev: EndpointID,
) -> CanonicalBlock {
    CanonicalBlockBuilder::default()
        .block_type(PREVIOUS_NODE_BLOCK)
        .block_number(block_number)
        .block_control_flags(bcf)
        .data(CanonicalData::PreviousNode(prev))
        .build()
        .unwrap()
}

pub fn new_bundle_age_block(
    block_number: u64,
    bcf: BlockControlFlags,
    time: u64,
) -> CanonicalBlock {
    CanonicalBlockBuilder::default()
        .block_type(BUNDLE_AGE_BLOCK)
        .block_number(block_number)
        .block_control_flags(bcf)
        .data(CanonicalData::BundleAge(time))
        .build()
        .unwrap()
}
