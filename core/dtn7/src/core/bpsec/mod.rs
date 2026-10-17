//! BPSec (RFC 9172) security policies: configured security rules, their keys and their
//! enforcement in the bundle processing.
//!
//! BPSec is inactive without configured rules. The security operations themselves are
//! provided by bp7.

pub mod config;
#[cfg(feature = "bpsec")]
pub mod enforcement;
#[cfg(feature = "bpsec")]
pub mod keys;
#[cfg(feature = "bpsec")]
pub mod policy;
pub mod statistics;

use anyhow::Result;
use bp7::administrative_record::StatusReportReason;
use bp7::Bundle;
use config::{BpsecConfig, SecurityRule};
use log::info;
use serde::Serialize;
use statistics::RuleStatistics;

#[cfg(feature = "bpsec")]
use {
    parking_lot::{Mutex, RwLock},
    statistics::Statistics,
    std::sync::Arc,
};

#[cfg(feature = "bpsec")]
lazy_static::lazy_static! {
    /// Keys of the configured key directory, see keys::KeyStore
    pub static ref KEYS: keys::KeyStore = keys::KeyStore::default();
    /// Security rules in use, set by init()
    static ref RULES: RwLock<Arc<Vec<SecurityRule>>> = RwLock::default();
    /// What the security rules did since init()
    static ref STATISTICS: Mutex<Statistics> = Mutex::default();
}

/// Where in the bundle processing the security rules are enforced
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingStep {
    /// A local application sends a bundle
    Transmit,
    /// A bundle was received from a peer
    Receive,
    /// A bundle is forwarded to a peer
    Forward,
}

/// Outcome of enforcing the security rules on a bundle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Continue processing the bundle, which security operations may have changed
    Process,
    /// Delete the bundle for the given status report reason
    Drop(StatusReportReason),
}

/// Validates the BPSec configuration, loads the keys and activates the security rules.
pub fn init(config: &BpsecConfig) -> Result<()> {
    config.validate()?;
    #[cfg(feature = "bpsec")]
    {
        if config.is_active() {
            let loaded = KEYS.reload(config)?;
            info!(
                "BPSec: {} security rules, {} keys loaded",
                config.rules.len(),
                loaded
            );
        } else {
            info!("BPSec: inactive, no security rules configured");
        }
        *RULES.write() = Arc::new(config.rules.clone());
        *STATISTICS.lock() = Statistics::default();
        Ok(())
    }
    #[cfg(not(feature = "bpsec"))]
    {
        if config.is_active() {
            anyhow::bail!(
                "bpsec rules are configured, but dtn7 was built without the bpsec feature"
            );
        }
        info!("BPSec: inactive, no security rules configured");
        Ok(())
    }
}

/// Enforces the security rules on a bundle at a processing step, changing the bundle
/// as the security operations demand. See enforcement::enforce.
pub fn enforce(step: ProcessingStep, bundle: &mut Bundle) -> Verdict {
    #[cfg(feature = "bpsec")]
    {
        let rules = RULES.read().clone();
        if rules.is_empty() {
            return Verdict::Process;
        }
        let node = crate::CONFIG.lock().host_eid.clone();
        // counted per bundle, so the global statistics are not locked during the
        // security operations
        let mut statistics = Statistics::default();
        let verdict = enforcement::enforce(
            &rules,
            &KEYS.current(),
            &node,
            step,
            bundle,
            &mut statistics,
        );
        STATISTICS.lock().merge(&statistics);
        verdict
    }
    #[cfg(not(feature = "bpsec"))]
    {
        let _ = (step, bundle);
        Verdict::Process
    }
}

/// State of the security rules, without key material
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub active: bool,
    /// Number of loaded keys
    pub keys: usize,
    pub rules: Vec<RuleStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleStatus {
    #[serde(flatten)]
    pub rule: SecurityRule,
    pub statistics: RuleStatistics,
}

/// Returns the security rules in use with their statistics.
pub fn status() -> Status {
    #[cfg(feature = "bpsec")]
    {
        let rules = RULES.read().clone();
        let statistics = STATISTICS.lock().clone();
        Status {
            active: !rules.is_empty(),
            keys: KEYS.current().len(),
            rules: rules
                .iter()
                .map(|rule| RuleStatus {
                    rule: rule.clone(),
                    statistics: statistics.rule(&rule.name),
                })
                .collect(),
        }
    }
    #[cfg(not(feature = "bpsec"))]
    Status {
        active: false,
        keys: 0,
        rules: Vec::new(),
    }
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
