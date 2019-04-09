pub mod dummy_cl;

pub mod stcp;

use crate::core::core::DtnCore;
use crate::dtnd::daemon::DtnCmd;
use std::fmt::{Debug, Display};
use std::sync::mpsc::Sender;

pub trait ConvergencyLayerAgent: Debug + Send + Display {
    fn setup(&mut self, tx: Sender<DtnCmd>);
    fn scheduled_process(&self, core: &DtnCore);
}
