//! Keys for BPSec security operations, loaded from a directory and replaceable at
//! runtime.

use super::config::{BpsecConfig, SecurityRule};
use anyhow::{bail, Context, Result};
use log::{debug, warn};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use zeroize::Zeroizing;

/// File extension of key files, other files in the key directory are ignored.
const KEY_FILE_EXTENSION: &str = "key";

/// A secret key, zeroized when dropped and never printed.
pub struct Key(Zeroizing<Vec<u8>>);

impl Key {
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Key(<redacted>, {} bytes)", self.0.len())
    }
}

/// A set of keys by key id.
#[derive(Debug, Default)]
pub struct KeySet {
    keys: HashMap<String, Key>,
}

impl KeySet {
    /// Loads all `<key id>.key` files of a directory, each containing a hex encoded key.
    pub fn load(dir: &Path) -> Result<KeySet> {
        let mut keys = HashMap::new();
        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("could not read key directory {}", dir.display()))?;
        for entry in entries {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some(KEY_FILE_EXTENSION) {
                debug!("Ignoring {} in key directory", path.display());
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                warn!("Ignoring key file with invalid name: {}", path.display());
                continue;
            };
            warn_if_readable_by_others(&path);
            let key = read_key(&path)?;
            keys.insert(id.to_string(), key);
        }
        Ok(KeySet { keys })
    }

    pub fn get(&self, id: &str) -> Option<&Key> {
        self.keys.get(id)
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Checks that all keys used by the rules exist and fit their security context.
    pub fn check_rules(&self, rules: &[SecurityRule]) -> Result<()> {
        for rule in rules {
            let Some(key) = self.get(&rule.key) else {
                bail!("bpsec rule {} uses unknown key {}", rule.name, rule.key);
            };
            if let Some(len) = rule.required_key_len() {
                if key.bytes().len() != len {
                    bail!(
                        "bpsec rule {} requires a key of {} bytes, key {} has {} bytes",
                        rule.name,
                        len,
                        rule.key,
                        key.bytes().len()
                    );
                }
            }
        }
        Ok(())
    }
}

fn read_key(path: &Path) -> Result<Key> {
    let content = Zeroizing::new(
        std::fs::read_to_string(path)
            .with_context(|| format!("could not read key file {}", path.display()))?,
    );
    let hex = content.trim();
    // unhexify does not reject odd lengths
    if hex.is_empty() || hex.len() % 2 != 0 {
        bail!(
            "key file {} does not contain a hex encoded key",
            path.display()
        );
    }
    let bytes = bp7::helpers::unhexify(hex)
        .map_err(|_| anyhow::anyhow!("key file {} is not hex encoded", path.display()))?;
    Ok(Key(Zeroizing::new(bytes)))
}

#[cfg(unix)]
fn warn_if_readable_by_others(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.permissions().mode() & 0o077 != 0 {
            warn!(
                "Key file {} is accessible by other users, consider `chmod 600`",
                path.display()
            );
        }
    }
}

#[cfg(not(unix))]
fn warn_if_readable_by_others(_path: &Path) {}

/// The key set in use. A reload replaces it as a whole, so a security operation
/// working on a snapshot from current() never sees a mix of old and new keys.
#[derive(Debug, Default)]
pub struct KeyStore {
    current: RwLock<Arc<KeySet>>,
}

impl KeyStore {
    pub fn current(&self) -> Arc<KeySet> {
        self.current.read().clone()
    }

    /// Loads the keys of the configured key directory and checks them against the
    /// rules. On errors, the current keys are kept. Returns the number of loaded keys.
    pub fn reload(&self, config: &BpsecConfig) -> Result<usize> {
        let Some(dir) = &config.keys else {
            bail!("no bpsec key directory configured");
        };
        let keys = KeySet::load(dir)?;
        keys.check_rules(&config.rules)?;
        let len = keys.len();
        *self.current.write() = Arc::new(keys);
        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::bpsec::config::*;
    use std::path::PathBuf;

    fn key_dir(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            std::fs::write(dir.path().join(name), content).unwrap();
        }
        dir
    }

    fn rule(operation: SecurityOperation, key: &str) -> SecurityRule {
        SecurityRule {
            name: "test".into(),
            role: SecurityRole::Acceptor,
            source: "*".into(),
            destination: "*".into(),
            operation,
            targets: vec![SecurityTarget::Payload],
            key: key.into(),
            context: SecurityContext::default(),
            on_failure: FailureAction::Drop,
        }
    }

    fn config(dir: &Path, rules: Vec<SecurityRule>) -> BpsecConfig {
        BpsecConfig {
            keys: Some(PathBuf::from(dir)),
            rules,
        }
    }

    #[test]
    fn keys_are_loaded_from_key_files() {
        let dir = key_dir(&[("hmac.key", "00112233\n"), ("notes.txt", "not a key")]);
        let keys = KeySet::load(dir.path()).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys.get("hmac").unwrap().bytes(), &[0x00, 0x11, 0x22, 0x33]);
        assert!(keys.get("notes").is_none());
    }

    #[test]
    fn invalid_key_files_are_rejected() {
        for content in ["", "abc", "zz", "00 11"] {
            let dir = key_dir(&[("broken.key", content)]);
            assert!(KeySet::load(dir.path()).is_err(), "accepted {:?}", content);
        }
        assert!(KeySet::load(Path::new("/nonexistent/dtn7/keys")).is_err());
    }

    #[test]
    fn keys_are_not_printed() {
        let dir = key_dir(&[("secret.key", "deadbeef")]);
        let keys = KeySet::load(dir.path()).unwrap();
        let printed = format!("{:?}", keys);
        // neither as hex nor as bytes (0xde = 222, 0xad = 173)
        assert!(!printed.contains("deadbeef"), "{}", printed);
        assert!(!printed.contains("222, 173"), "{}", printed);
        assert!(printed.contains("redacted"));
    }

    #[test]
    fn rules_need_existing_keys_of_the_right_length() {
        let aes256 = "00".repeat(32);
        let dir = key_dir(&[("hmac.key", "0011"), ("aes.key", &aes256)]);
        let keys = KeySet::load(dir.path()).unwrap();

        assert!(keys
            .check_rules(&[rule(SecurityOperation::Bib, "hmac")])
            .is_ok());
        assert!(keys
            .check_rules(&[rule(SecurityOperation::Bcb, "aes")])
            .is_ok());
        assert!(keys
            .check_rules(&[rule(SecurityOperation::Bib, "missing")])
            .is_err());
        // default AES-256-GCM needs 32 bytes
        assert!(keys
            .check_rules(&[rule(SecurityOperation::Bcb, "hmac")])
            .is_err());

        let mut aes128 = rule(SecurityOperation::Bcb, "aes");
        aes128.context.aes_variant = Some(AesVariant::Aes128);
        assert!(keys.check_rules(&[aes128]).is_err());
    }

    #[test]
    fn failed_reload_keeps_current_keys() {
        let dir = key_dir(&[("hmac.key", "0011")]);
        let store = KeyStore::default();
        let rules = vec![rule(SecurityOperation::Bib, "hmac")];
        assert_eq!(store.reload(&config(dir.path(), rules.clone())).unwrap(), 1);
        let before = store.current();

        // a broken key file and a missing key both fail the reload
        std::fs::write(dir.path().join("hmac.key"), "zz").unwrap();
        assert!(store.reload(&config(dir.path(), rules.clone())).is_err());
        std::fs::write(dir.path().join("hmac.key"), "2233").unwrap();
        let missing = vec![rule(SecurityOperation::Bib, "missing")];
        assert!(store.reload(&config(dir.path(), missing)).is_err());
        assert!(Arc::ptr_eq(&before, &store.current()));

        assert_eq!(store.reload(&config(dir.path(), rules)).unwrap(), 1);
        assert_eq!(store.current().get("hmac").unwrap().bytes(), &[0x22, 0x33]);
        // the old snapshot is unchanged
        assert_eq!(before.get("hmac").unwrap().bytes(), &[0x00, 0x11]);
    }
}
