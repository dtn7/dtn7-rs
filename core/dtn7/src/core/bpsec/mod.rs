//! BPSec (RFC 9172) security policies: configured security rules and their keys.
//!
//! BPSec is inactive without configured rules. The security operations themselves are
//! provided by bp7.

pub mod config;

use anyhow::Result;
use config::BpsecConfig;

/// Validates the BPSec configuration at startup.
pub fn init(config: &BpsecConfig) -> Result<()> {
    config.validate()?;
    if !config.is_active() {
        log::info!("BPSec: inactive, no security rules configured");
        return Ok(());
    }
    #[cfg(feature = "bpsec")]
    {
        log::info!("BPSec: {} security rules", config.rules.len());
        Ok(())
    }
    #[cfg(not(feature = "bpsec"))]
    anyhow::bail!("bpsec rules are configured, but dtn7 was built without the bpsec feature")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_without_rules_is_inactive() {
        assert!(init(&BpsecConfig::default()).is_ok());
    }
}
