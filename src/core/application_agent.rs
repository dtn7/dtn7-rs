use bp7::{Bundle, EndpointID};
use std::collections::VecDeque;
use std::fmt::Debug;

pub trait ApplicationAgent: Debug {
    fn eid(&self) -> &EndpointID;
    fn push(&mut self, bundle: &Bundle);
    fn pop(&mut self) -> Option<Bundle>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApplicationAgentData {
    eid: EndpointID,
    bundles: VecDeque<Bundle>,
}

impl ApplicationAgent for ApplicationAgentData {
    fn eid(&self) -> &EndpointID {
        &self.eid
    }
    fn push(&mut self, bundle: &Bundle) {
        println!("Received {:?}", bundle);
        self.bundles.push_back(bundle.clone());
    }
    fn pop(&mut self) -> Option<Bundle> {
        self.bundles.pop_front()
    }
}

impl ApplicationAgentData {
    pub fn new_with(eid: EndpointID) -> ApplicationAgentData {
        ApplicationAgentData {
            eid,
            bundles: VecDeque::new(),
        }
    }
}
