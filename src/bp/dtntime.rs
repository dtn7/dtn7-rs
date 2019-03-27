use chrono::prelude::DateTime;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub type DtnTime = u64;

const SECONDS1970_TO2K: u64 = 946_684_800;
pub const DTN_TIME_EPOCH: DtnTime = 0;

pub trait DtnTimeHelpers {
    fn unix(self) -> u64;
    fn string(self) -> String;
}

impl DtnTimeHelpers for DtnTime {
    /// Convert to unix timestamp.
    fn unix(self) -> u64 {
        self + SECONDS1970_TO2K
    }

    /// Convert to human readable rfc3339 compliant time string.
    fn string(self) -> String {
        let d = UNIX_EPOCH + Duration::from_secs(self + SECONDS1970_TO2K);
        let datetime = DateTime::<Utc>::from(d);
        datetime.to_rfc3339()
    }
}

/// Get current time as DtnTime timestamp
pub fn dtn_time_now() -> DtnTime {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards!!")
        .as_secs()
        - SECONDS1970_TO2K
}

/// Timestamp when a bundle was created, consisting of the DtnTime and a sequence number.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)] // hacked struct as tuple because bug in serialize_tuple
pub struct CreationTimestamp(DtnTime, u64);

impl fmt::Display for CreationTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.0.string(), self.1)
    }
}

impl CreationTimestamp {
    pub fn new() -> CreationTimestamp {
        Default::default()
    }
    pub fn with_time_and_seq(t: DtnTime, seqno: u64) -> CreationTimestamp {
        CreationTimestamp(t, seqno)
    }
    pub fn get_seqno(&self) -> u64 {
        self.1
    }
    pub fn get_dtntime(&self) -> DtnTime {
        self.0
    }
}
