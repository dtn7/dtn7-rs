use bp7::{Bundle, EndpointID};
use log::{debug, info};
use std::collections::VecDeque;
use std::fmt::Debug;

pub trait ApplicationAgent: Debug {
    fn eid(&self) -> &EndpointID;
    fn push(&mut self, bundle: &Bundle);
    fn pop(&mut self) -> Option<Bundle>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleApplicationAgent {
    eid: EndpointID,
    bundles: VecDeque<Bundle>,
}

impl ApplicationAgent for SimpleApplicationAgent {
    fn eid(&self) -> &EndpointID {
        &self.eid
    }
    fn push(&mut self, bundle: &Bundle) {
        info!("Received {:?} | {:?}", bundle.id(), bp7::dtn_time_now());
        debug!("Received raw: {:?}", bundle);
        self.bundles.push_back(bundle.clone());
    }
    fn pop(&mut self) -> Option<Bundle> {
        self.bundles.pop_front()
    }
}

impl SimpleApplicationAgent {
    pub fn new_with(eid: EndpointID) -> SimpleApplicationAgent {
        SimpleApplicationAgent {
            eid,
            bundles: VecDeque::new(),
        }
    }
}
