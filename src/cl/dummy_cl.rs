use crate::core::core::{ConversionLayer, DtnCore};
use crate::dtnd::daemon::DtnCmd;
use log::{debug, error, info, trace, warn};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Default)]
pub struct DummyConversionLayer {
    counter: u64,
}

impl DummyConversionLayer {
    pub fn new() -> DummyConversionLayer {
        DummyConversionLayer { counter: 0 }
    }
}
impl ConversionLayer for DummyConversionLayer {
    fn setup(&mut self, tx: Sender<DtnCmd>) {
        debug!("Setup Dummy Conversion Layer");
    }
    fn scheduled_send(&self, core: &DtnCore) {
        debug!("Scheduled send Dummy Conversion Layer");
    }
}

impl std::fmt::Display for DummyConversionLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "DummyConversionLayer")
    }
}
