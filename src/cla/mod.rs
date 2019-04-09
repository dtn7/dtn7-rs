pub mod dummy_cl;

pub mod stcp;

use crate::core::core::DtnCore;
use bp7::ByteBuffer;
use std::fmt::{Debug, Display};

pub trait ConvergencyLayerAgent: Debug + Send + Display {
    fn setup(&mut self);
    fn scheduled_process(&self, ready: &Vec<ByteBuffer>, keys: &Vec<String>);
}
