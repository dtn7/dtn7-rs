use core::fmt;
use serde::de::{SeqAccess, Visitor};
use serde::ser::{SerializeSeq, Serializer};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::{fmt::Debug, time::Duration};

use crate::ipnd::services::ServiceBlock;
use bp7::{bundle::Block, ByteBuffer, EndpointID};

// Draft IPND version is 0x04
// This implementation uses more enhanced and additional features, also the bundle protocol version
// was updated to 7, therefore the version is set to 0x07
pub const IPND_VERSION: u8 = 0x07;

// Constants + Type for Beacon flags

pub type BeaconFlags = u8;

/// Source EID of sender node is present (should always be set)
pub const SOURCE_EID_PRESENT: BeaconFlags = 0b0000_0001;

/// Service block is present
pub const SERVICE_BLOCK_PRESENT: BeaconFlags = 0b0000_0010;

/// Beacon Period field is present
pub const BEACON_PERIOD_PRESENT: BeaconFlags = 0b0000_0100;

/// Bits 4 - 7 are reserved for future specifications
pub const RESERVED_BITS: BeaconFlags = 0b1111_1000;

/// The struct representing the messages sent from a node to advertise itself in an unknown neighbourhood
///
/// Based on RFC5050 with changes to the encoding. Old encoding was based on SDNV, new encoding uses CBOR
#[derive(Debug, Clone, PartialEq)]
pub struct Beacon {
    /// Mandatory, 8-bit field describing the version of the IPND service that constructed this beacon, draft version = 0x04, this version = 0x07
    version: u8,

    /// Mandatory, 8-bit flag field.
    flags: BeaconFlags,

    /// Mandatory, Endpoint identifier of the node the beacon was send from
    eid: EndpointID,

    /// 32-bit field incremented once for each beacon transmitted to the same IP address
    beacon_sequence_number: u32,

    /// Optional, announces additional services and ConvergencyLayerAgents
    service_block: ServiceBlock,

    /// Optional, indicates the current senders beacon interval in seconds
    beacon_period: Option<Duration>,
}

impl Beacon {
    /// Method to create a default Beacon that doesn't advertise any services or future beacons
    ///
    /// Comes with an empty ServiceBlock and no beacon_period
    pub fn new(eid: EndpointID) -> Beacon {
        Beacon {
            version: IPND_VERSION,
            flags: SOURCE_EID_PRESENT,
            eid,
            beacon_sequence_number: 0,
            service_block: ServiceBlock::new(),
            beacon_period: None,
        }
    }

    /// Creates a new Beacon with pre-configured EID, ServiceBlock and BeaconPeriod
    pub fn with_config(
        eid: EndpointID,
        service_block: ServiceBlock,
        beacon_period: Option<Duration>,
    ) -> Beacon {
        let mut beacon = Beacon {
            version: IPND_VERSION,
            flags: SOURCE_EID_PRESENT,
            eid,
            beacon_sequence_number: 0,
            service_block,
            beacon_period,
        };

        if !beacon.service_block().is_empty() {
            beacon.add_flags(SERVICE_BLOCK_PRESENT);
        }
        if let Some(_bp) = beacon.beacon_period() {
            beacon.add_flags(BEACON_PERIOD_PRESENT);
        }

        beacon
    }

    /// Returns the currently used IPND version
    pub fn version(&self) -> String {
        format!("{:#x}", self.version)
    }

    /// Returns the current flag configuration
    pub fn flags(&self) -> String {
        format!("{:#010b}", self.flags)
    }

    /// Returns the sender eid
    pub fn eid(&self) -> &EndpointID {
        &self.eid
    }

    /// Returns the amount of times this beacon was send to the same IP address
    pub fn beacon_sequence_number(&self) -> u32 {
        self.beacon_sequence_number
    }

    /// Returns the ServiceBlock
    pub fn service_block(&self) -> &ServiceBlock {
        &self.service_block
    }

    /// Returns the BeaconPeriod (if present)
    pub fn beacon_period(&self) -> Option<Duration> {
        self.beacon_period
    }

    /// Adds flags with bitwise OR-Operation
    ///
    /// Is used when there are operations performed on the beacon that require a flag change. E.g.
    ///
    /// - Adding the canonical EID
    /// - Adding the ServiceBlock
    /// - Adding the BeaconPeriod
    fn add_flags(&mut self, flags: BeaconFlags) {
        self.flags |= flags;
    }

    /// Sets the beacon_sequence_number
    pub fn set_beacon_sequence_number(&mut self, bsn: u32) {
        self.beacon_sequence_number = bsn;
    }

    /// This method adds a cla to the corresponding vector of the ServiceBlock
    pub fn add_cla(&mut self, name: &str, port: &Option<u16>) {
        self.service_block.add_cla(name, port);
        self.add_flags(SERVICE_BLOCK_PRESENT);
    }

    /// This method adds a custom service to the corresponding HashMap of the ServiceBlock
    pub fn add_custom_service(&mut self, tag: u8, service: String) {
        let payload = ServiceBlock::build_custom_service(tag, service.as_str())
            .expect("Error while parsing Service to byte format");
        self.service_block.add_custom_service(tag, &payload.1);
        self.add_flags(SERVICE_BLOCK_PRESENT);
    }
}

// Everything below this comment is about implementing traits for the Beacon struct

// Implementation of the Display trait for Beacons for proper formatting
impl std::fmt::Display for Beacon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let temp = format!("{:010b}", self.flags);
        let output = if self.beacon_period.is_some() {
            format!("Version: {:#x}\tFlags: {}\tBeaconSequenceNumber: {}\nEID: {}\nServiceBlock:\n{}\nBeaconPeriod: {:#?}",
        self.version, temp, self.beacon_sequence_number, self.eid, self.service_block, self.beacon_period.unwrap())
        } else {
            format!("Version: {:#x}\tFlags: {}\tBeaconSequenceNumber: {}\nEID: {}\nServiceBlock:\n{}\nBeaconPeriod: None",
        self.version, temp, self.beacon_sequence_number, self.eid, self.service_block)
        };

        write!(f, "{}", output)
    }
}

impl Serialize for Beacon {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let num_elems = if self.beacon_period.is_some() && !self.service_block.is_empty() {
            // Amount of elements inside a Beacon with a BeaconPeriod
            // and a ServiceBlock that contains at least one Service
            6
        } else if self.beacon_period.is_none() && self.service_block.is_empty() {
            // Amount of elements inside a Beacon without BeaconPeriod
            // and empty ServiceBlock
            4
        } else {
            // Amount of elements inside a Beacon without a BeaconPeriod
            // and a ServiceBlock that contains at least one Service

            // Amount of elements inside a Beacon with a BeaconPeriod
            // and an empty ServiceBlock
            5
        };

        let mut seq = serializer.serialize_seq(Some(num_elems))?;

        seq.serialize_element(&self.version)?;
        seq.serialize_element(&self.flags)?;
        seq.serialize_element(&self.eid)?;
        seq.serialize_element(&self.beacon_sequence_number)?;
        if !self.service_block.is_empty() {
            seq.serialize_element(&self.service_block)?;
        }
        if self.beacon_period.is_some() {
            let period_number = self.beacon_period.unwrap().as_secs();
            seq.serialize_element(&period_number)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Beacon {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BeaconVisitor;

        impl<'de> Visitor<'de> for BeaconVisitor {
            type Value = Beacon;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("beacon")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let version = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let flags = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let eid = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let beacon_sequence_number = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &self))?;

                // If there are no more elements inside the sequence the received Beacon did not contain
                // a BeaconPeriod and a ServiceBlock
                if seq.size_hint().unwrap() == 0 {
                    Ok(Beacon {
                        version,
                        flags,
                        eid,
                        beacon_sequence_number,
                        service_block: ServiceBlock::new(),
                        beacon_period: None,
                    })

                // If there is exactly one element left inside the sequence it has to be either a BeaconPeriod or a ServiceBlock
                // Check for it by looking at the flags
                } else if seq.size_hint().unwrap() == 1 {
                    if (flags & SERVICE_BLOCK_PRESENT) == SERVICE_BLOCK_PRESENT {
                        Ok(Beacon {
                            version,
                            flags,
                            eid,
                            beacon_sequence_number,
                            service_block: seq.next_element()?.unwrap(),
                            beacon_period: None,
                        })
                    } else {
                        Ok(Beacon {
                            version,
                            flags,
                            eid,
                            beacon_sequence_number,
                            service_block: ServiceBlock::new(),
                            beacon_period: Some(Duration::from_secs(seq.next_element()?.unwrap())),
                        })
                    }
                } else {
                    // Default branch executed when a 'full' Beacon is received, meaning a Beacon with BeaconPeriod AND ServiceBlock
                    let service_block = seq.next_element()?.unwrap();
                    let beacon_period = Some(Duration::from_secs(seq.next_element()?.unwrap()));
                    Ok(Beacon {
                        version,
                        flags,
                        eid,
                        beacon_sequence_number,
                        service_block,
                        beacon_period,
                    })
                }
            }
        }

        deserializer.deserialize_any(BeaconVisitor)
    }
}

// Shortcut method to serialize a Beacon
impl Block for Beacon {
    fn to_cbor(&self) -> ByteBuffer {
        serde_cbor::to_vec(&self).expect("Error exporting Beacon to cbor")
    }
}
