use super::bundle::*;
use serde::{Deserialize, Serialize};
use std::convert::From;
use std::fmt;
use url::Url;

/******************************
 *
 * Endpoint ID
 *
 ******************************/

// TODO: implement IPN uri scheme
pub const ENDPOINT_URI_SCHEME_DTN: u8 = 1;
pub const ENDPOINT_URI_SCHEME_IPN: u8 = 2;

pub const DTN_NONE: EndpointID = EndpointID::DtnNone(ENDPOINT_URI_SCHEME_DTN, 0);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpnAddress(pub u32, pub u32);

/// # Examples
///
/// ```
/// use dtn7::bp::eid::*;
///
/// let cbor_eid = [130, 1, 106, 110, 111, 100, 101, 49, 47, 116, 101, 115, 116];
/// let deserialized: EndpointID = serde_cbor::from_slice(&cbor_eid).unwrap();
/// assert_eq!(deserialized, EndpointID::Dtn(ENDPOINT_URI_SCHEME_DTN, "node1/test".to_string()))
///
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum EndpointID {
    DtnNone(u8, u8),
    Dtn(u8, String),
    Ipn(u8, IpnAddress),
}

impl Default for EndpointID {
    fn default() -> Self {
        EndpointID::DtnNone(ENDPOINT_URI_SCHEME_DTN, 0)
    }
}
impl EndpointID {
    pub fn new() -> EndpointID {
        Default::default()
    }
    /// # Examples
    ///
    /// ```
    /// use dtn7::bp::eid::*;
    ///
    /// assert_eq!(EndpointID::with_dtn("node1".to_string()),EndpointID::Dtn(ENDPOINT_URI_SCHEME_DTN,"node1".to_string()));
    ///
    /// assert_eq!(EndpointID::with_dtn("node1/endpoint1".to_string()),EndpointID::Dtn(ENDPOINT_URI_SCHEME_DTN,"node1/endpoint1".to_string()));
    /// ```
    pub fn with_dtn(addr: String) -> EndpointID {
        EndpointID::Dtn(ENDPOINT_URI_SCHEME_DTN, addr)
    }
    /// # Examples
    ///
    /// ```
    /// use dtn7::bp::eid::*;
    ///
    /// assert_eq!(EndpointID::with_dtn_none(), EndpointID::DtnNone(ENDPOINT_URI_SCHEME_DTN,0));
    /// ```
    pub fn with_dtn_none() -> EndpointID {
        EndpointID::DtnNone(ENDPOINT_URI_SCHEME_DTN, 0)
    }
    /// # Examples
    ///
    /// ```
    /// use dtn7::bp::eid::*;
    ///
    /// assert_eq!(EndpointID::with_ipn( IpnAddress(23, 42) ), EndpointID::Ipn(ENDPOINT_URI_SCHEME_IPN, IpnAddress(23, 42)) );
    /// ```
    pub fn with_ipn(addr: IpnAddress) -> EndpointID {
        EndpointID::Ipn(ENDPOINT_URI_SCHEME_IPN, addr)
    }

    pub fn get_scheme(&self) -> String {
        match self {
            EndpointID::DtnNone(_, _) => "dtn".to_string(),
            EndpointID::Dtn(_, _) => "dtn".to_string(),
            EndpointID::Ipn(_, _) => "ipn".to_string(),
        }
    }
    pub fn get_scheme_specific_part_dtn(&self) -> Option<String> {
        match self {
            EndpointID::Dtn(_, ssp) => Some(ssp.to_string()),
            _ => None,
        }
    }
    pub fn to_string(&self) -> String {
        let result = format!(
            "{}://{}",
            self.get_scheme(),
            self.get_scheme_specific_part_dtn()
                .unwrap_or_else(|| "none".to_string())
        );
        result
    }
}

impl fmt::Display for EndpointID {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl EndpointID {
    /// # Examples
    ///
    /// ```
    /// use dtn7::bp::eid::*;
    ///
    /// let eid = EndpointID::DtnNone(1, 0);
    /// assert_eq!(eid.validation_error().is_none(), true); // should not fail
    ///
    /// let eid = EndpointID::DtnNone(0, 0);
    /// assert_eq!(eid.validation_error().is_some(), true); // should fail   
    /// let eid = EndpointID::DtnNone(1, 1);
    /// assert_eq!(eid.validation_error().is_some(), true); // should fail   
    ///
    /// let eid = EndpointID::Ipn(2, IpnAddress(23, 42));
    /// assert_eq!(eid.validation_error().is_none(), true); // should not fail
    /// let eid = EndpointID::Ipn(1, IpnAddress(23, 42));
    /// assert_eq!(eid.validation_error().is_some(), true); // should fail   
    /// let eid = EndpointID::Ipn(2, IpnAddress(0, 0));
    /// assert_eq!(eid.validation_error().is_some(), true); // should fail   
    /// ```
    pub fn validation_error(&self) -> Option<Bp7Error> {
        match self {
            EndpointID::Dtn(_, _) => None, // TODO: Implement validation for dtn scheme
            EndpointID::Ipn(code, addr) => {
                if *code != ENDPOINT_URI_SCHEME_IPN {
                    Some(Bp7Error::EIDError(
                        "Wrong URI scheme code for IPN".to_string(),
                    ))
                } else if addr.0 < 1 || addr.1 < 1 {
                    Some(Bp7Error::EIDError(
                        "IPN's node and service number must be >= 1".to_string(),
                    ))
                } else {
                    None
                }
            }
            EndpointID::DtnNone(code, addr) => {
                if *code != ENDPOINT_URI_SCHEME_DTN {
                    Some(Bp7Error::EIDError(
                        "Wrong URI scheme code for DTN".to_string(),
                    ))
                } else if *addr != 0 {
                    Some(Bp7Error::EIDError(
                        "dtn none must have uint(0) set as address".to_string(),
                    ))
                } else {
                    None
                }
            }
        }
    }
}

/// Load EndpointID from URL string.
/// Support for IPN and dtn schemes.
///
/// # Examples
///
/// ```
/// use dtn7::bp::eid::*;
///
/// let eid = EndpointID::from("dtn://none".to_string());
/// assert_eq!(eid, EndpointID::DtnNone(ENDPOINT_URI_SCHEME_DTN, 0));
///
/// let eid = EndpointID::from("dtn:none".to_string());
/// assert_eq!(eid, EndpointID::DtnNone(ENDPOINT_URI_SCHEME_DTN, 0));
///
/// let eid = EndpointID::from("dtn://node1/endpoint1".to_string());
/// assert_eq!(eid, EndpointID::Dtn(ENDPOINT_URI_SCHEME_DTN, "node1/endpoint1".to_string()));
///
/// let eid = EndpointID::from("dtn:node1/endpoint1".to_string());
/// assert_eq!(eid, EndpointID::Dtn(ENDPOINT_URI_SCHEME_DTN, "node1/endpoint1".to_string()));
///   
/// ```
impl From<String> for EndpointID {
    fn from(item: String) -> Self {
        let item = if item.contains("://") {
            item
        } else {
            item.replace(":", "://")
        };
        let u = Url::parse(&item).unwrap();
        println!("{}", u.scheme());
        let host = u.host_str().unwrap();

        match u.scheme() {
            "dtn" => {
                if host == "none" {
                    return <EndpointID>::with_dtn_none();
                }
                let mut host = format!("{}{}", host, u.path());
                if host.ends_with('/') {
                    host.truncate(host.len() - 1);
                }
                EndpointID::with_dtn(host)
            }
            "ipn" => {
                let fields: Vec<&str> = host.split('.').collect();
                if fields.len() != 2 {
                    panic!("wrong number of fields in IPN address");
                }
                let p1: u32 = fields[0].parse().unwrap();
                let p2: u32 = fields[1].parse().unwrap();

                EndpointID::with_ipn(IpnAddress(p1, p2))
            }
            _ => <EndpointID>::with_dtn_none(),
        }
    }
}
