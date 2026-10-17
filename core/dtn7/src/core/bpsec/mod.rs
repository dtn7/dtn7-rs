//! BPSec (RFC 9172) security policies: configured security rules and their keys.
//!
//! BPSec is inactive without configured rules. The security operations themselves are
//! provided by bp7.

pub mod config;
#[cfg(feature = "bpsec")]
pub mod keys;
#[cfg(feature = "bpsec")]
pub mod policy;

use anyhow::Result;
use config::BpsecConfig;

#[cfg(feature = "bpsec")]
lazy_static::lazy_static! {
    /// Keys of the configured key directory, see keys::KeyStore
    pub static ref KEYS: keys::KeyStore = keys::KeyStore::default();
}

/// Validates the BPSec configuration and loads the keys at startup.
pub fn init(config: &BpsecConfig) -> Result<()> {
    config.validate()?;
    if !config.is_active() {
        log::info!("BPSec: inactive, no security rules configured");
        return Ok(());
    }
    #[cfg(feature = "bpsec")]
    {
        let loaded = KEYS.reload(config)?;
        log::info!(
            "BPSec: {} security rules, {} keys loaded",
            config.rules.len(),
            loaded
        );
        Ok(())
    }
    #[cfg(not(feature = "bpsec"))]
    anyhow::bail!("bpsec rules are configured, but dtn7 was built without the bpsec feature")
}

#[cfg(test)]
mod tests {
    use super::config::*;
    use super::*;

    #[test]
    fn init_without_rules_is_inactive() {
        assert!(init(&BpsecConfig::default()).is_ok());
    }

    #[test]
    fn init_fails_for_rules_without_usable_keys() {
        let config = BpsecConfig {
            keys: Some("/nonexistent/dtn7/keys".into()),
            rules: vec![SecurityRule {
                name: "accept".into(),
                role: SecurityRole::Acceptor,
                source: "*".into(),
                destination: "*".into(),
                operation: SecurityOperation::Bib,
                targets: vec![SecurityTarget::Payload],
                key: "hmac".into(),
                context: SecurityContext::default(),
                on_failure: FailureAction::Drop,
            }],
        };
        assert!(init(&config).is_err());
    }
}
