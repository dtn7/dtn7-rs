use super::bundle::*;
use super::crc::*;
use super::dtntime::*;
use super::eid::*;
use derive_builder::Builder;

use serde::{Deserialize, Serialize};

/******************************
 *
 * Primary Block
 *
 ******************************/

//#[derive(Debug, Serialize_tuple, Deserialize_tuple, Clone)]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Builder)]
#[builder(default)]
pub struct PrimaryBlock {
    version: DtnVersionType,
    pub bundle_control_flags: BundleControlFlags,
    pub crc_type: CRCType,
    pub destination: EndpointID,
    pub source: EndpointID,
    pub report_to: EndpointID,
    pub creation_timestamp: CreationTimestamp,
    pub lifetime: LifetimeType,
    pub fragmentation_offset: FragOffsetType,
    pub total_data_length: TotalDataLengthType,
    crc: ByteBuffer,
}

impl Default for PrimaryBlock {
    fn default() -> Self {
        PrimaryBlock::new()
    }
}
impl PrimaryBlock {
    pub fn new() -> PrimaryBlock {
        PrimaryBlock {
            version: DTN_VERSION,
            bundle_control_flags: 0,
            crc_type: CRC_NO,
            destination: EndpointID::new(),
            source: EndpointID::new(),
            report_to: EndpointID::new(),
            creation_timestamp: CreationTimestamp::new(),
            lifetime: 0,
            fragmentation_offset: 0,
            total_data_length: 0,
            crc: Vec::new(),
        }
    }
    pub fn has_fragmentation(&self) -> bool {
        self.bundle_control_flags.has(BUNDLE_IS_FRAGMENT)
    }
    pub fn validation_errors(&self) -> Option<Bp7ErrorList> {
        let mut errors: Bp7ErrorList = Vec::new();

        if self.version != DTN_VERSION {
            errors.push(Bp7Error::PrimaryBlockError(format!(
                "Wrong version, {} instead of {}",
                self.version, DTN_VERSION
            )));
        }

        // bundle control flags
        if let Some(mut err) = self.bundle_control_flags.validation_errors() {
            errors.append(&mut err);
        }

        if let Some(chk_err) = self.destination.validation_error() {
            errors.push(chk_err);
        }

        if let Some(chk_err) = self.source.validation_error() {
            errors.push(chk_err);
        }
        if let Some(chk_err) = self.report_to.validation_error() {
            errors.push(chk_err);
        }

        if !errors.is_empty() {
            return Some(errors);
        }
        None
    }
}
impl From<BlockVariants> for PrimaryBlock {
    fn from(item: BlockVariants) -> Self {
        match item {
            BlockVariants::NotFragmentedAndNoCrc(
                version,
                bundle_control_flags,
                crc_type,
                destination,
                source,
                report_to,
                creation_timestamp,
                lifetime,
            ) => PrimaryBlock {
                version,
                bundle_control_flags,
                crc_type,
                destination: destination.clone(),
                source: source.clone(),
                report_to: report_to.clone(),
                creation_timestamp: creation_timestamp.clone(),
                lifetime,
                fragmentation_offset: 0,
                total_data_length: 0,
                crc: Vec::new(),
            },
            BlockVariants::JustFragmented(
                version,
                bundle_control_flags,
                crc_type,
                destination,
                source,
                report_to,
                creation_timestamp,
                lifetime,
                fragmentation_offset,
                total_data_length,
            ) => PrimaryBlock {
                version,
                bundle_control_flags,
                crc_type,
                destination: destination.clone(),
                source: source.clone(),
                report_to: report_to.clone(),
                creation_timestamp: creation_timestamp.clone(),
                lifetime,
                fragmentation_offset,
                total_data_length,
                crc: Vec::new(),
            },
            BlockVariants::JustCrc(
                version,
                bundle_control_flags,
                crc_type,
                destination,
                source,
                report_to,
                creation_timestamp,
                lifetime,
                crc,
            ) => PrimaryBlock {
                version,
                bundle_control_flags,
                crc_type,
                destination: destination.clone(),
                source: source.clone(),
                report_to: report_to.clone(),
                creation_timestamp: creation_timestamp.clone(),
                lifetime,
                fragmentation_offset: 0,
                total_data_length: 0,
                crc: crc.clone(),
            },
            BlockVariants::FragmentedAndCrc(
                version,
                bundle_control_flags,
                crc_type,
                destination,
                source,
                report_to,
                creation_timestamp,
                lifetime,
                fragmentation_offset,
                total_data_length,
                crc,
            ) => PrimaryBlock {
                version,
                bundle_control_flags,
                crc_type,
                destination: destination.clone(),
                source: source.clone(),
                report_to: report_to.clone(),
                creation_timestamp: creation_timestamp.clone(),
                lifetime,
                fragmentation_offset,
                total_data_length,
                crc: crc.clone(),
            },
            _ => panic!("Error in parsing primary block"),
        }
    }
}
impl Block for PrimaryBlock {
    fn to_variant(&self) -> BlockVariants {
        if self.crc_type == CRC_NO && !self.has_fragmentation() {
            BlockVariants::NotFragmentedAndNoCrc(
                self.version,
                self.bundle_control_flags,
                self.crc_type,
                self.destination.clone(),
                self.source.clone(),
                self.report_to.clone(),
                self.creation_timestamp.clone(),
                self.lifetime,
            )
        } else if self.crc_type == CRC_NO && self.has_fragmentation() {
            BlockVariants::JustFragmented(
                self.version,
                self.bundle_control_flags,
                self.crc_type,
                self.destination.clone(),
                self.source.clone(),
                self.report_to.clone(),
                self.creation_timestamp.clone(),
                self.lifetime,
                self.fragmentation_offset,
                self.total_data_length,
            )
        } else if self.crc_type != CRC_NO && !self.has_fragmentation() {
            BlockVariants::JustCrc(
                self.version,
                self.bundle_control_flags,
                self.crc_type,
                self.destination.clone(),
                self.source.clone(),
                self.report_to.clone(),
                self.creation_timestamp.clone(),
                self.lifetime,
                self.crc.clone(),
            )
        } else {
            BlockVariants::FragmentedAndCrc(
                self.version,
                self.bundle_control_flags,
                self.crc_type,
                self.destination.clone(),
                self.source.clone(),
                self.report_to.clone(),
                self.creation_timestamp.clone(),
                self.lifetime,
                self.fragmentation_offset,
                self.total_data_length,
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
pub fn new_primary_block(
    dst: String,
    src: String,
    creation_timestamp: CreationTimestamp,
    lifetime: u64,
) -> PrimaryBlock {
    let dst_eid = EndpointID::from(dst);
    let src_eid = EndpointID::from(src);

    let pb = PrimaryBlock {
        version: DTN_VERSION,
        bundle_control_flags: 0,
        crc_type: CRC_NO,
        destination: dst_eid,
        source: src_eid.clone(),
        report_to: src_eid,
        creation_timestamp: creation_timestamp,
        lifetime,
        fragmentation_offset: 0,
        total_data_length: 0,
        crc: Vec::new(),
    };
    let serialized = serde_cbor::to_vec(&pb).unwrap();
    println!("serialized = {:?}", serialized);

    /*let deserialized: PrimaryBlock = serde_cbor::from_slice(&serialized).unwrap();
    println!("deserialized = {:?}", deserialized);*/

    let serialized = serde_json::to_string(&pb).unwrap();
    println!("serialized = {:?}", serialized);
    /*
    let deserialized: PrimaryBlock = serde_json::from_str(&serialized).unwrap();
    println!("deserialized = {:?}", deserialized);*/
    pb
}
