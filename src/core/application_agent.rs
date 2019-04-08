use bp7::{Bundle, EndpointID};
use std::fmt::Debug;

pub trait ApplicationAgent: Debug {
    fn eid(&self) -> &EndpointID;
    fn deliver(&self, bundle: &Bundle);
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApplicationAgentData {
    eid: EndpointID,
}

impl ApplicationAgent for ApplicationAgentData {
    fn eid(&self) -> &EndpointID {
        &self.eid
    }
    fn deliver(&self, bundle: &Bundle) {
        println!("Received {:?}", bundle);
    }
}

impl ApplicationAgentData {
    pub fn new_with(eid: EndpointID) -> ApplicationAgentData {
        ApplicationAgentData { eid }
    }
}
