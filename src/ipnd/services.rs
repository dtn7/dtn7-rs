extern crate alloc;
use core::fmt;
use log::info;
use serde::de::{SeqAccess, Visitor};
use serde::ser::{SerializeSeq, Serializer};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;

/// Struct representing the ServiceBlock used in Beacons to advertise additional services
///
/// Contains two vectors,
///
/// one reserved for ConvergencyLayerAgents
///
/// and one for user defined services
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceBlock {
    clas: Vec<(String, Option<u16>)>,
    services: HashMap<u8, Vec<u8>>,
}
impl Default for ServiceBlock {
    fn default() -> Self {
        Self::new()
    }
}
impl ServiceBlock {
    /// Creates a new ServiceBlock without any services or clas
    pub fn new() -> ServiceBlock {
        ServiceBlock {
            clas: Vec::new(),
            services: HashMap::new(),
        }
    }

    /// Returns the vector of ConvergencyLayerAgents
    pub fn clas(&self) -> &Vec<(String, Option<u16>)> {
        &self.clas
    }

    /// Converts services into the format used by IPND
    pub fn convert_services(&self) -> HashMap<u8, String> {
        let mut convert: HashMap<u8, String> = HashMap::new();
        for (tag, payload) in &self.services {
            match *tag {
                Service::CUSTOM_STRING => {
                    convert.insert(
                        *tag,
                        String::from_utf8(payload.clone())
                            .expect("Error parsing string from bytebuffer"),
                    );
                }
                Service::GEO_LOCATION => {
                    let latitude: f32 =
                        f32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                    let longitude: f32 =
                        f32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                    convert.insert(*tag, format!("{} {}", latitude, longitude));
                }
                Service::BATTERY => {
                    let int: i8 = i8::from_be_bytes([payload[0]]);
                    convert.insert(*tag, format!("{}", int));
                }
                Service::ADDRESS => {
                    let message = String::from_utf8(payload.clone())
                        .expect("Couldn't parse byte array into string");
                    convert.insert(*tag, message);
                }
                _ => {
                    info!("Unknown Service encountered. Compare senders IPND version with this one to check for incompatibilities.");
                }
            }
        }
        convert
    }

    /// Returns the vector of user defined services
    pub fn services(&self) -> &HashMap<u8, Vec<u8>> {
        &self.services
    }

    /// This method adds a cla to the corresponding vector of a ServiceBlock
    pub fn add_cla(&mut self, name: &str, port: &Option<u16>) {
        self.clas.push((name.to_owned(), *port))
    }

    /// This method adds a custom service to the HashMap of a ServiceBlock
    pub fn add_custom_service(&mut self, tag: u8, service: &[u8]) {
        self.services.insert(tag, service.to_owned());
    }
    /// This method sets the clas vector of a ServiceBlock to the one provided
    pub fn set_clas(&mut self, clas: Vec<(String, Option<u16>)>) {
        self.clas = clas;
    }
    /// This method sets the services hashmap of a ServiceBlock to the one provided
    pub fn set_services(&mut self, services: HashMap<u8, Vec<u8>>) {
        self.services = services;
    }

    /// Method to build custom services
    ///
    /// Performs checks on tag and payload combinations
    ///
    /// to make sure that the tag and payload content match
    pub fn build_custom_service(tag: u8, payload: &str) -> Result<(u8, Vec<u8>), String> {
        match tag {
            // CustomString to allow a random unformatted string
            Service::CUSTOM_STRING => {
                if payload.as_bytes().len() > 64 {
                    Err(String::from(
                        "The provided custom message is to big. Aim for less than 64 characters",
                    ))
                } else {
                    Ok((tag, payload.as_bytes().to_vec()))
                }
            }
            // GeoLocation expects two floats to represent geographical location (Latitude/Longitude)
            Service::GEO_LOCATION => {
                let input: Vec<&str> = payload.split_whitespace().collect();
                if input.len() < 2 {
                    Err(String::from(
                        "Not enough arguments provided to represent geographical location",
                    ))
                } else {
                    let first: f32 = input[0].parse().expect("Couldn't parse latitude");
                    let second: f32 = input[1].parse().expect("Couldn't parse longitude");
                    let mut bytes = first.to_be_bytes().to_vec();
                    bytes.extend(second.to_be_bytes().to_vec().iter());
                    Ok((tag, bytes))
                }
            }
            // Battery expect an integer between 0 and 100 to represent battery level in %
            Service::BATTERY => {
                let res = payload.parse::<i8>();
                if let Ok(input) = res {
                    if !(0..=100).contains(&input) {
                        Err(String::from("Provided number can not be used to represent battery level. Please provide a number between 0 and 100"))
                    } else {
                        Ok((tag, input.to_be_bytes().to_vec()))
                    }
                } else {
                    Err(format!(
                        "Could not parse provided argument into an integer. {}",
                        res.expect_err("")
                    ))
                }
            }
            // TODO: refactor this to geolocation
            // Address expects 5 arguments String Int Int String String to represent an address
            Service::ADDRESS => {
                //let input: Vec<&str> = payload.split_whitespace().collect();
                if payload.split_whitespace().count() == 5 {
                    Ok((tag, payload.as_bytes().to_vec()))
                } else {
                    Err(String::from("Can not derive address from provided arguments. Argument order is: Street HouseNumber PostalNumber City CountryCode"))
                }
            }
            // Undefined tags
            _ => Err(String::from(
                "This custom tag is not yet defined. Please refrain from using it until added.",
            )),
        }
    }

    /// Check if the ServiceBlock contains no CLAs and no other Services
    pub fn is_empty(&self) -> bool {
        self.clas.len() + self.services.len() == 0
    }
}

// Implementation of the Display trait for ServiceBlocks for proper formatting
impl std::fmt::Display for ServiceBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut output = String::new();
        output.push_str("ConvergencyLayerAgents:\n");
        let mut counter = 0;
        for (name, port) in self.clas() {
            let str = if port.is_some() {
                format!(
                    "{}. CLA: Name = {} Port = {}\n",
                    counter,
                    name,
                    port.unwrap()
                )
            } else {
                format!("{}. CLA: Name = {}\n", counter, name)
            };
            output.push_str(str.as_str());
            counter += 1;
        }
        counter = 0;
        output.push_str("Other services:\n");
        for (tag, payload) in self.services() {
            let str = match *tag {
                Service::CUSTOM_STRING => format!(
                    "{}. Tag = {} Custom String Message: {}\n",
                    counter,
                    tag,
                    String::from_utf8(payload.clone())
                        .expect("Error parsing string from bytebuffer")
                ),
                Service::GEO_LOCATION => {
                    let latitude: f32 =
                        f32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                    let longitude: f32 =
                        f32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                    format!("{}. Tag = {} Geographic location service. Current location at: Latitude {} Longitude {}\n",
                            counter, tag, latitude, longitude)
                }
                Service::BATTERY => {
                    let int: i8 = i8::from_be_bytes([payload[0]]);
                    format!(
                        "{}. Tag = {} Battery service. Battery level at {}%\n",
                        counter, tag, int
                    )
                }
                Service::ADDRESS => {
                    let message = String::from_utf8(payload.clone())
                        .expect("Couldn't parse byte array into string");
                    let address: Vec<&str> = message.split_whitespace().collect();
                    format!("{}. Tag = {} Address service. Street {}; House Number {}; Postal Number {}; City {}; Country Code {}\n",
                            counter, tag, address[0],address[1],address[2],address[3],address[4])
                }
                _ => {
                    info!("Unknown Service encountered. Compare senders IPND version with this one to check for incompatibilities.");
                    format!("")
                }
            };
            output.push_str(str.as_str());
            counter += 1;
        }
        output.pop();
        write!(f, "{}", output)
    }
}

impl Serialize for ServiceBlock {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // If the ServiceBlock is empty there is nothing to serialize, else the amount of elements to serialize is equal to
        // the amount of elements inside both vectors of the ServiceBlock
        let num_elems = if self.is_empty() { 0 } else { 2 };
        let mut seq = serializer.serialize_seq(Some(num_elems))?;
        if num_elems > 0 {
            seq.serialize_element(&self.clas)?;
            seq.serialize_element(&self.services)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for ServiceBlock {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ServiceBlockVisitor;

        impl<'de> Visitor<'de> for ServiceBlockVisitor {
            type Value = ServiceBlock;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("ServiceBlock")
            }
            fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                if seq.size_hint().unwrap() < 1 {
                    Ok(ServiceBlock::new())
                } else {
                    let clas = seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                    let services = seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                    let mut service_block = ServiceBlock::new();
                    service_block.set_clas(clas);
                    service_block.set_services(services);
                    Ok(service_block)
                }
            }
        }

        deserializer.deserialize_any(ServiceBlockVisitor)
    }
}

/// Enum struct for defining services
struct Service;

impl Service {
    pub const CUSTOM_STRING: u8 = 63;
    pub const GEO_LOCATION: u8 = 127;
    pub const BATTERY: u8 = 191;
    pub const ADDRESS: u8 = 255;
}
