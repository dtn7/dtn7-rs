pub mod dummy_cl;

pub mod stcp;

use bp7::ByteBuffer;
use std::fmt::{Debug, Display};

pub trait ConvergencyLayerAgent: Debug + Send + Display {
    fn setup(&mut self);
    fn scheduled_process(&self, ready: &[ByteBuffer], keys: &Vec<String>);
}
