use serde::{Deserialize, Serialize};
use std::fmt;

use derive_builder::Builder;

use super::canonical::*;
use super::crc::*;
use super::dtntime::*;
use super::eid::*;
use super::primary::*;

/// Version for upcoming bundle protocol standard is 7.
pub const DTN_VERSION: u32 = 7;

pub type ByteBuffer = Vec<u8>;

pub type DtnVersionType = u32;
pub type CanonicalBlockNumberType = u64;
pub type FragOffsetType = u64;
pub type LifetimeType = u64;
pub type TotalDataLengthType = u64;

#[derive(Debug, Clone)]
pub enum Bp7Error {
    CanonicalBlockError(String),
    PrimaryBlockError(String),
    EIDError(String),
    DtnTimeError(String),
    CrcError(String),
    BundleError(String),
    StcpError(String),
    BundleControlFlagError(String),
    BlockControlFlagError(String),
}

impl fmt::Display for Bp7Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        /*match self {
            CanonicalBlockError(err) => {
                write!(f, "CanonicalBlock: {:?}", self, )
            }
        } */

        write!(f, "{:?}", self)
        // or, alternatively:
        // fmt::Debug::fmt(self, f)
    }
}

pub type Bp7ErrorList = Vec<Bp7Error>;

/******************************
 *
 * Block
 *
 ******************************/

pub trait Block: Clone {
    /// Convert block struct to a serializable enum
    fn to_variant(&self) -> BlockVariants;
    fn has_crc(&self) -> bool;
    fn calculate_crc(&mut self) {
        let new_crc = calculate_crc(self);
        self.set_crc(new_crc);
    }
    fn check_crc(&self) -> bool {
        check_crc(self)
    }
    /// Reset crc field to an empty value
    fn reset_crc(&mut self) {
        let crc_type = self.crc_type();
        let empty = empty_crc(crc_type).unwrap();
        self.set_crc(empty);
    }
    fn set_crc_type(&mut self, crc_type: CRCType);
    fn crc_type(&self) -> CRCType;
    fn crc(&self) -> ByteBuffer;
    fn set_crc(&mut self, crc: ByteBuffer);

    /// Convert block to a cbor encoded byte vector
    fn to_cbor(&self) -> ByteBuffer {
        let packet = self.to_variant();
        serde_cbor::to_vec(&packet).unwrap()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum BlockVariants {
    NotFragmentedAndNoCrc(
        DtnVersionType,
        BundleControlFlags,
        CRCType,
        EndpointID,
        EndpointID,
        EndpointID,
        CreationTimestamp,
        LifetimeType,
    ),
    JustCrc(
        DtnVersionType,
        BundleControlFlags,
        CRCType,
        EndpointID,
        EndpointID,
        EndpointID,
        CreationTimestamp,
        LifetimeType,
        #[serde(with = "serde_bytes")] ByteBuffer,
    ),
    JustFragmented(
        DtnVersionType,
        BundleControlFlags,
        CRCType,
        EndpointID,
        EndpointID,
        EndpointID,
        CreationTimestamp,
        LifetimeType,
        FragOffsetType,
        TotalDataLengthType,
    ),
    FragmentedAndCrc(
        DtnVersionType,
        BundleControlFlags,
        CRCType,
        EndpointID,
        EndpointID,
        EndpointID,
        CreationTimestamp,
        LifetimeType,
        FragOffsetType,
        TotalDataLengthType,
        #[serde(with = "serde_bytes")] ByteBuffer,
    ),
    CanonicalWithoutCrc(
        CanonicalBlockType,
        CanonicalBlockNumberType,
        BlockControlFlags,
        CRCType,
        CanonicalData,
    ),
    Canonical(
        CanonicalBlockType,
        CanonicalBlockNumberType,
        BlockControlFlags,
        CRCType,
        CanonicalData,
        #[serde(with = "serde_bytes")] ByteBuffer,
    ),
}

/******************************
 *
 * Block Control Flags
 *
 ******************************/

pub type BlockControlFlags = u8;

/// Bundle must be deleted if this block can't be processed.
pub const BLOCK_DELETE_BUNDLE: BlockControlFlags = 0x08;

/// Transmission of a status report is requested if this block can't be processed.
pub const BLOCK_STATUS_REPORT: BlockControlFlags = 0x04;

/// Block must be removed from the bundle if it can't be processed.
pub const BLOCK_REMOVE: BlockControlFlags = 0x02;

/// This block must be replicated in every fragment.
pub const BLOCK_REPLICATE: BlockControlFlags = 0x01;

pub const BLOCK_CFRESERVED_FIELDS: BlockControlFlags = 0xF0;

pub trait BlockValidation {
    fn has(self, flag: BlockControlFlags) -> bool;
    fn validation_error(self) -> Option<Bp7Error>;
}
impl BlockValidation for BlockControlFlags {
    fn has(self, flag: BlockControlFlags) -> bool {
        (self & flag) != 0
    }
    fn validation_error(self) -> Option<Bp7Error> {
        if self.has(BLOCK_CFRESERVED_FIELDS) {
            return Some(Bp7Error::BlockControlFlagError(
                "Given flag contains reserved bits".to_string(),
            ));
        }
        None
    }
}

/******************************
 *
 * Bundle Control Flags
 *
 ******************************/

pub type BundleControlFlags = u16;

/// Request reporting of bundle deletion.
pub const BUNDLE_STATUS_REQUEST_DELETION: BundleControlFlags = 0x1000;

/// Request reporting of bundle delivery.
pub const BUNDLE_STATUS_REQUEST_DELIVERY: BundleControlFlags = 0x0800;

/// Request reporting of bundle forwarding.
pub const BUNDLE_STATUS_REQUEST_FORWARD: BundleControlFlags = 0x0400;

/// Request reporting of bundle reception.
pub const BUNDLE_STATUS_REQUEST_RECEPTION: BundleControlFlags = 0x0100;

/// The bundle contains a "manifest" extension block.
pub const BUNDLE_CONTAINS_MANIFEST: BundleControlFlags = 0x0080;

/// Status time is requested in all status reports.
pub const BUNDLE_REQUEST_STATUS_TIME: BundleControlFlags = 0x0040;

///Acknowledgment by the user application is requested.
pub const BUNDLE_REQUEST_USER_APPLICATION_ACK: BundleControlFlags = 0x0020;

/// The bundle must not be fragmented.
pub const BUNDLE_MUST_NOT_FRAGMENTED: BundleControlFlags = 0x0004;

/// The bundle's payload is an administrative record.
pub const BUNDLE_ADMINISTRATIVE_RECORD_PAYLOAD: BundleControlFlags = 0x0002;

/// The bundle is a fragment.
pub const BUNDLE_IS_FRAGMENT: BundleControlFlags = 0x0001;

pub const BUNDLE_CFRESERVED_FIELDS: BundleControlFlags = 0xE218;

pub trait BundleValidation {
    fn has(self, flag: BundleControlFlags) -> bool;
    fn validation_errors(self) -> Option<Bp7ErrorList>;
}
impl BundleValidation for BundleControlFlags {
    fn has(self, flag: BundleControlFlags) -> bool {
        (self & flag) != 0
    }
    fn validation_errors(self) -> Option<Bp7ErrorList> {
        let mut errors: Bp7ErrorList = Vec::new();
        if self.has(BUNDLE_CFRESERVED_FIELDS) {
            errors.push(Bp7Error::BundleControlFlagError(
                "Given flag contains reserved bits".to_string(),
            ));
        }
        if self.has(BUNDLE_IS_FRAGMENT) && self.has(BUNDLE_MUST_NOT_FRAGMENTED) {
            errors.push(Bp7Error::BundleControlFlagError(
                "Both 'bundle is a fragment' and 'bundle must not be fragmented' flags are set"
                    .to_string(),
            ));
        }
        let admin_rec_check = !self.has(BUNDLE_ADMINISTRATIVE_RECORD_PAYLOAD)
            || (!self.has(BUNDLE_STATUS_REQUEST_RECEPTION)
                && !self.has(BUNDLE_STATUS_REQUEST_FORWARD)
                && !self.has(BUNDLE_STATUS_REQUEST_DELIVERY)
                && !self.has(BUNDLE_STATUS_REQUEST_DELETION));
        if !admin_rec_check {
            errors.push(Bp7Error::BundleControlFlagError(
                "\"payload is administrative record => no status report request flags\" failed"
                    .to_string(),
            ))
        }
        if !errors.is_empty() {
            return Some(errors);
        }
        None
    }
}

/******************************
 *
 * Bundle
 *
 ******************************/

/// Bundle represents a bundle as defined in section 4.2.1. Each Bundle contains
/// one primary block and multiple canonical blocks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Builder)]
#[builder(default)]
pub struct Bundle {
    pub primary: PrimaryBlock,
    pub canonicals: Vec<CanonicalBlock>,
}

impl Default for Bundle {
    fn default() -> Self {
        Bundle {
            primary: PrimaryBlock::new(),
            canonicals: Vec::new(),
        }
    }
}
impl Bundle {
    pub fn new(primary: PrimaryBlock, canonicals: Vec<CanonicalBlock>) -> Bundle {
        Bundle {
            primary: primary,
            canonicals: canonicals,
        }
    }
    /// Validate bundle and optionally return list of errors.
    pub fn validation_errors(&self) -> Option<Bp7ErrorList> {
        let mut errors: Bp7ErrorList = Vec::new();

        if let Some(mut err) = self.primary.validation_errors() {
            errors.append(&mut err);
        }
        for blck in &self.canonicals {
            if let Some(mut err) = blck.validation_errors() {
                errors.append(&mut err);
            }
        }
        if self
            .primary
            .bundle_control_flags
            .has(BUNDLE_ADMINISTRATIVE_RECORD_PAYLOAD)
            || self.primary.source == DTN_NONE
        {
            for cb in &self.canonicals {
                if cb.block_control_flags.has(BLOCK_STATUS_REPORT) {
                    errors.push(Bp7Error::BundleError(
                        "Bundle Processing Control Flags indicate that this bundle's payload is an administrative record or the source node is omitted, but the \"Transmit status report if block cannot be processed\" Block Processing Control Flag was set in a Canonical Block".to_string()
                    ));
                }
            }
        }
        let block_numbers = self
            .canonicals
            .iter()
            .map(|e| e.block_number)
            .collect::<Vec<CanonicalBlockNumberType>>();

        let block_types = self
            .canonicals
            .iter()
            .map(|e| e.block_number)
            .collect::<Vec<CanonicalBlockNumberType>>();

        if (1..block_numbers.len()).any(|i| block_numbers[i..].contains(&block_numbers[i - 1])) {
            errors.push(Bp7Error::BundleError(
                "Block numbers occurred multiple times".to_string(),
            ));
        }

        if block_types
            .iter()
            .filter(|&i| *i == BUNDLE_AGE_BLOCK)
            .count()
            > 1
            || block_types
                .iter()
                .filter(|&i| *i == PREVIOUS_NODE_BLOCK)
                .count()
                > 1
            || block_types
                .iter()
                .filter(|&i| *i == HOP_COUNT_BLOCK)
                .count()
                > 1
        {
            errors.push(Bp7Error::BundleError(
                "PreviousNode, BundleAge and HopCound blocks must not occure multiple times"
                    .to_string(),
            ));
        }
        if self.primary.creation_timestamp.get_dtntime() == 0
            && !block_types.contains(&BUNDLE_AGE_BLOCK)
        {
            errors.push(Bp7Error::BundleError(
                "Creation Timestamp is zero, but no Bundle Age block is present".to_string(),
            ));
        }
        if !errors.is_empty() {
            return Some(errors);
        }
        None
    }
    pub fn is_administrative_record(&self) -> bool {
        self.primary
            .bundle_control_flags
            .has(BUNDLE_ADMINISTRATIVE_RECORD_PAYLOAD)
    }
    /// Sets the given CRCType for each block. The crc value
    /// is calculated on-the-fly before serializing.
    pub fn set_crc(&mut self, crc_type: CRCType) {
        self.primary.set_crc_type(crc_type);
        for b in &mut self.canonicals {
            b.set_crc_type(crc_type);
        }
    }
    /// Check whether a bundle has only valid CRC checksums in all blocks.
    pub fn crc_valid(&self) -> bool {
        if !self.primary.check_crc() {
            return false;
        }
        for b in &self.canonicals {
            if !b.check_crc() {
                return false;
            }
        }
        true
    }
    /// Calculate crc for all blocks.
    pub fn calculate_crc(&mut self) {
        self.primary.calculate_crc();
        for b in &mut self.canonicals {
            b.calculate_crc();
        }
    }

    fn blocks(&mut self) -> Vec<BlockVariants> {
        let mut blocks: Vec<BlockVariants> = Vec::new();
        self.primary.calculate_crc();
        blocks.push(self.primary.to_variant());
        for b in &mut self.canonicals {
            //dbg!(b.get_data());
            b.calculate_crc();
            blocks.push(b.to_variant());
        }
        blocks
    }
    pub fn extension_block(
        &mut self,
        block_type: CanonicalBlockType,
    ) -> Option<(&mut CanonicalBlock)> {
        for b in &mut self.canonicals {
            if b.block_type == block_type && b.extension_validation_error().is_none() {
                //let cdata = b.get_data().clone();
                return Some(b);
            }
        }
        None
    }
    /// Serialize bundle as CBOR encoded byte buffer.
    pub fn to_cbor(&mut self) -> ByteBuffer {
        self.calculate_crc();
        let mut bytebuf = serde_cbor::to_vec(&self.blocks()).unwrap();
        bytebuf[0] = 0x9f; // TODO: fix hack, indefinite-length array encoding
        bytebuf.push(0xff); // break mark
        bytebuf
    }
    /// Serialize bundle as JSON encoded string.
    pub fn to_json(&mut self) -> String {
        self.calculate_crc();
        serde_json::to_string(&self.blocks()).unwrap()
    }

    /// ID returns a kind of uniquene representation of this bundle, containing
    /// the souce node and creation timestamp. If this bundle is a fragment, the
    /// offset is also present.
    pub fn id(&self) -> String {
        let mut id = format!(
            "{}-{}-{}-{}",
            self.primary.source,
            self.primary.creation_timestamp.get_dtntime(),
            self.primary.creation_timestamp.get_seqno(),
            self.primary.destination
        );
        if self.primary.has_fragmentation() {
            id = format!("{}-{}", id, self.primary.fragmentation_offset);
        }
        id
    }
}

/// Deserialize from CBOR byte buffer.
impl From<ByteBuffer> for Bundle {
    fn from(item: ByteBuffer) -> Self {
        let deserialized: Vec<BlockVariants> = serde_cbor::from_slice(&item).unwrap();
        let prim = PrimaryBlock::from(deserialized[0].clone()); // TODO: dirty hack and not idiomatic
        let mut cblocks: Vec<CanonicalBlock> = Vec::new();
        for b in deserialized.iter().skip(1) {
            cblocks.push(CanonicalBlock::from(b.clone()));
        }
        Bundle::new(prim, cblocks)
    }
}

/// Deserialize from JSON string.
impl From<String> for Bundle {
    fn from(item: String) -> Self {
        let deserialized: Vec<BlockVariants> = serde_json::from_str(&item).unwrap();
        let prim = PrimaryBlock::from(deserialized[0].clone()); // TODO: dirty hack and not idiomatic
        let mut cblocks: Vec<CanonicalBlock> = Vec::new();
        for b in deserialized.iter().skip(1) {
            cblocks.push(CanonicalBlock::from(b.clone()));
        }
        Bundle::new(prim, cblocks)
    }
}
