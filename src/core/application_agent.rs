use bp7::{Bundle, EndpointID};
use enum_dispatch::enum_dispatch;
use log::{debug, info};
use std::collections::VecDeque;
use std::fmt::Debug;
use tokio::sync::mpsc::Sender;

use crate::dtnd::ws::BundleDelivery;
//use crate::dtnd::ws::WsAASession;

#[enum_dispatch]
#[derive(Debug)]
pub enum ApplicationAgentEnum {
    SimpleApplicationAgent,
}

#[enum_dispatch(ApplicationAgentEnum)]
pub trait ApplicationAgent: Debug {
    fn eid(&self) -> &EndpointID;
    fn push(&mut self, bundle: &Bundle);
    fn pop(&mut self) -> Option<Bundle>;
    fn set_delivery_addr(&mut self, addr: Sender<BundleDelivery>);
    fn clear_delivery_addr(&mut self);
    fn delivery_addr(&self) -> Option<Sender<BundleDelivery>>;
}

#[derive(Debug, Clone)]
pub struct SimpleApplicationAgent {
    eid: EndpointID,
    bundles: VecDeque<Bundle>,
    delivery: Option<Sender<BundleDelivery>>,
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

            if addr.try_send(BundleDelivery { 0: bundle.clone() }).is_err() {
                self.bundles.push_back(bundle.clone());
            }
        } else {
            // save in temp buffer for delivery
            self.bundles.push_back(bundle.clone());
        }
    }
    fn pop(&mut self) -> Option<Bundle> {
        self.bundles.pop_front()
    }

    fn set_delivery_addr(&mut self, addr: Sender<BundleDelivery>) {
        self.delivery = Some(addr);
    }

    fn clear_delivery_addr(&mut self) {
        self.delivery = None;
    }

    fn delivery_addr(&self) -> Option<Sender<BundleDelivery>> {
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
