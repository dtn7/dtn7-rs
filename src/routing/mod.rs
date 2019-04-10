pub mod epidemic;
pub mod flooding;

use crate::cla::ConvergencyLayerAgent;
use bp7::{Bundle, ByteBuffer};
use std::fmt::Debug;
use std::fmt::Display;

pub trait RoutingAgent: Debug + Send + Display {
    fn route_bundle(
        &mut self,
        bundle: &ByteBuffer,
        peers: Vec<String>,
        cl_list: &[Box<dyn ConvergencyLayerAgent>],
    ) {
        unimplemented!();
    }
    fn route_all(
        &mut self,
        bundles: Vec<ByteBuffer>,
        peers: Vec<String>,
        cl_list: &[Box<dyn ConvergencyLayerAgent>],
    ) {
        unimplemented!();
    }
}
