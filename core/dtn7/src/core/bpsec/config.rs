//! Configuration of BPSec (RFC 9172) security rules, the `[bpsec]` section of the
//! configuration file.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// The `[bpsec]` configuration section. BPSec is inactive without rules.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BpsecConfig {
    /// Directory with one `<key id>.key` file per key, containing the hex encoded key.
    pub keys: Option<PathBuf>,
    #[serde(default, rename = "rule")]
    pub rules: Vec<SecurityRule>,
}

/// A security rule, applied to all bundles matching its source and destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SecurityRule {
    pub name: String,
    pub role: SecurityRole,
    /// Glob pattern for the bundle source, e.g., "dtn://node1/*". `*` matches within a
    /// path segment of the endpoint ID, `**` across segments.
    #[serde(default = "match_all")]
    pub source: String,
    /// Glob pattern for the bundle destination, see `source`
    #[serde(default = "match_all")]
    pub destination: String,
    pub operation: SecurityOperation,
    pub targets: Vec<SecurityTarget>,
    /// Id of the key, i.e., the name of its file in the key directory without `.key`
    pub key: String,
    #[serde(default)]
    pub context: SecurityContext,
    #[serde(default)]
    pub on_failure: FailureAction,
}

fn match_all() -> String {
    "**".to_string()
}

/// Role of this node in a security operation (RFC 9172, 2.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecurityRole {
    Source,
    Verifier,
    Acceptor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecurityOperation {
    /// Block integrity block with BIB-HMAC-SHA2 (RFC 9173, 3)
    Bib,
    /// Block confidentiality block with BCB-AES-GCM (RFC 9173, 4)
    Bcb,
}

/// Blocks that can be protected by a rule
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecurityTarget {
    Primary,
    Payload,
}

/// Security context parameters of a rule, each only valid for one operation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SecurityContext {
    /// Only for BIBs, HMAC-SHA-384 if not given
    pub sha_variant: Option<ShaVariant>,
    /// Only for BCBs, AES-256-GCM if not given
    pub aes_variant: Option<AesVariant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShaVariant {
    Sha256,
    Sha384,
    Sha512,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AesVariant {
    Aes128,
    Aes256,
}

impl AesVariant {
    pub fn key_len(&self) -> usize {
        match self {
            AesVariant::Aes128 => 16,
            AesVariant::Aes256 => 32,
        }
    }
}

/// What happens if a security operation fails
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureAction {
    /// Delete the bundle
    #[default]
    Drop,
    /// Remove the failed security operation, keeping its targets, only for verifiers and
    /// acceptors. The primary and payload blocks can not be removed, so removing the
    /// target is no option.
    RemoveOperation,
    /// Only log the failure and process the bundle as usual
    Log,
}

impl BpsecConfig {
    /// BPSec processing is only active with configured rules.
    pub fn is_active(&self) -> bool {
        !self.rules.is_empty()
    }

    /// Checks the rules for errors that do not depend on the keys.
    pub fn validate(&self) -> Result<()> {
        if self.is_active() && self.keys.is_none() {
            bail!("bpsec rules are configured, but no key directory (`keys`)");
        }
        let mut names = HashSet::new();
        for rule in &self.rules {
            if !names.insert(rule.name.as_str()) {
                bail!("bpsec rule names must be unique: {}", rule.name);
            }
            rule.validate()?;
        }
        Ok(())
    }
}

impl SecurityRule {
    fn validate(&self) -> Result<()> {
        let name = &self.name;
        if self.targets.is_empty() {
            bail!("bpsec rule {} has no targets", name);
        }
        let unique_targets: HashSet<_> = self.targets.iter().collect();
        if unique_targets.len() != self.targets.len() {
            bail!("bpsec rule {} lists a target more than once", name);
        }
        match self.operation {
            SecurityOperation::Bib if self.context.aes_variant.is_some() => {
                bail!("bpsec rule {}: aes-variant is only valid for BCBs", name)
            }
            SecurityOperation::Bcb if self.context.sha_variant.is_some() => {
                bail!("bpsec rule {}: sha-variant is only valid for BIBs", name)
            }
            // the primary block must not be encrypted (RFC 9172, 3.9)
            SecurityOperation::Bcb if self.targets.contains(&SecurityTarget::Primary) => {
                bail!("bpsec rule {}: BCBs can not target the primary block", name)
            }
            // a BCB can not be checked without decrypting it
            SecurityOperation::Bcb if self.role == SecurityRole::Verifier => {
                bail!(
                    "bpsec rule {}: BCBs can not be verified, only accepted",
                    name
                )
            }
            _ => {}
        }
        if self.role == SecurityRole::Source && self.on_failure == FailureAction::RemoveOperation {
            bail!(
                "bpsec rule {}: security sources can only drop or log on failures",
                name
            );
        }
        if self.operation == SecurityOperation::Bcb {
            bail!("bpsec rule {}: BCBs are not supported yet", name);
        }
        Ok(())
    }

    /// Length of the keys this rule requires, if the security context demands one.
    pub fn required_key_len(&self) -> Option<usize> {
        match self.operation {
            SecurityOperation::Bcb => Some(
                self.context
                    .aes_variant
                    .unwrap_or(AesVariant::Aes256)
                    .key_len(),
            ),
            SecurityOperation::Bib => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> Result<BpsecConfig> {
        let config = config::Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()?;
        Ok(config.get::<BpsecConfig>("bpsec")?)
    }

    const SIGN_RULE: &str = r#"
        [bpsec]
        keys = "/etc/dtn7/keys"

        [[bpsec.rule]]
        name = "sign-sensors"
        role = "source"
        destination = "dtn://*/sensors"
        operation = "bib"
        targets = ["payload", "primary"]
        key = "hmac-sensors"
        context = { sha-variant = "sha512" }
    "#;

    #[test]
    fn rules_are_parsed() {
        let config = parse(SIGN_RULE).unwrap();
        assert!(config.is_active());
        assert_eq!(config.keys, Some(PathBuf::from("/etc/dtn7/keys")));
        let rule = &config.rules[0];
        assert_eq!(rule.role, SecurityRole::Source);
        assert_eq!(rule.source, "**");
        assert_eq!(rule.destination, "dtn://*/sensors");
        assert_eq!(
            rule.targets,
            vec![SecurityTarget::Payload, SecurityTarget::Primary]
        );
        assert_eq!(rule.context.sha_variant, Some(ShaVariant::Sha512));
        assert_eq!(rule.on_failure, FailureAction::Drop);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn misspelled_settings_are_rejected() {
        let toml = SIGN_RULE.replace("context =", "contexts =");
        assert!(parse(&toml).is_err());
        let toml = SIGN_RULE.replace("\"bib\"", "\"bibs\"");
        assert!(parse(&toml).is_err());
    }

    #[test]
    fn invalid_rules_are_rejected() {
        let invalid = [
            // no key directory
            SIGN_RULE.replace("keys = \"/etc/dtn7/keys\"", ""),
            // no targets
            SIGN_RULE.replace("[\"payload\", \"primary\"]", "[]"),
            // duplicate targets
            SIGN_RULE.replace("[\"payload\", \"primary\"]", "[\"payload\", \"payload\"]"),
            // BIB with BCB parameters
            SIGN_RULE.replace("sha-variant = \"sha512\"", "aes-variant = \"aes128\""),
            // encrypted primary block
            SIGN_RULE
                .replace("\"bib\"", "\"bcb\"")
                .replace("context = { sha-variant = \"sha512\" }", ""),
            // BCBs can only be accepted
            SIGN_RULE
                .replace("\"source\"", "\"verifier\"")
                .replace("\"bib\"", "\"bcb\"")
                .replace("[\"payload\", \"primary\"]", "[\"payload\"]")
                .replace("context = { sha-variant = \"sha512\" }", ""),
            // BCBs are not supported yet
            SIGN_RULE
                .replace("\"source\"", "\"acceptor\"")
                .replace("\"bib\"", "\"bcb\"")
                .replace("[\"payload\", \"primary\"]", "[\"payload\"]")
                .replace("context = { sha-variant = \"sha512\" }", ""),
            // sources can not remove blocks
            SIGN_RULE.replace(
                "context =",
                "on-failure = \"remove-operation\"\n        context =",
            ),
        ];
        for toml in invalid {
            let config = parse(&toml).unwrap();
            assert!(config.validate().is_err(), "accepted: {}", toml);
        }

        let mut duplicate_names = parse(SIGN_RULE).unwrap();
        duplicate_names.rules.push(duplicate_names.rules[0].clone());
        assert!(duplicate_names.validate().is_err());
    }

    #[test]
    fn no_rules_means_inactive() {
        let config = parse("[bpsec]\n").unwrap();
        assert!(!config.is_active());
        assert!(config.validate().is_ok());
    }
}
