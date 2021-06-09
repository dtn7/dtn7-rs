use actix::Addr;
use bp7::{Bundle, EndpointID};
use log::{debug, info};
use std::collections::VecDeque;
use std::fmt::Debug;

use crate::dtnd::ws::BundleDelivery;
use crate::dtnd::ws::WsAASession;

pub trait ApplicationAgent: Debug {
    fn eid(&self) -> &EndpointID;
    fn push(&mut self, bundle: &Bundle);
    fn pop(&mut self) -> Option<Bundle>;
    fn set_delivery_addr(&mut self, addr: Addr<WsAASession>);
    fn clear_delivery_addr(&mut self);
    fn delivery_addr(&self) -> Option<Addr<WsAASession>>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleApplicationAgent {
    eid: EndpointID,
    bundles: VecDeque<Bundle>,
    delivery: Option<Addr<WsAASession>>,
}

impl ApplicationAgent for SimpleApplicationAgent {
    fn eid(&self) -> &EndpointID {
        &self.eid
    }
    fn push(&mut self, bundle: &Bundle) {
        info!("Received {:?} | {:?}", bundle.id(), bp7::dtn_time_now());
        debug!("Received raw: {:?}", bundle);

        // attempt direct delivery to websocket
        if let Some(addr) = self.delivery_addr() {
            // TODO: remove clone and work with reference
            addr.do_send(BundleDelivery { 0: bundle.clone() });
        } else {
            // save in temp buffer for delivery
            self.bundles.push_back(bundle.clone());
        }
    }
    fn pop(&mut self) -> Option<Bundle> {
        self.bundles.pop_front()
    }

    fn set_delivery_addr(&mut self, addr: Addr<WsAASession>) {
        self.delivery = Some(addr);
    }

    fn clear_delivery_addr(&mut self) {
        self.delivery = None;
    }

    fn delivery_addr(&self) -> Option<Addr<WsAASession>> {
        self.delivery.clone()
    }
}

impl SimpleApplicationAgent {
    pub fn with(eid: EndpointID) -> SimpleApplicationAgent {
        SimpleApplicationAgent {
            eid,
            bundles: VecDeque::new(),
            delivery: None,
        }
    }
}
