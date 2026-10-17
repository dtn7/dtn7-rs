//! Counters of the security operations per security rule, shown by `/status/bpsec`.

use serde::Serialize;
use std::collections::HashMap;

/// What a security rule did with a bundle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Signed,
    Verified,
    Accepted,
    Encrypted,
    Decrypted,
    /// The rule demanded a security operation the bundle lacks
    Missing,
    /// A security operation failed
    Failed,
    /// The bundle was dropped due to a missing or failed operation
    Dropped,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RuleStatistics {
    pub signed: u64,
    pub verified: u64,
    pub accepted: u64,
    pub encrypted: u64,
    pub decrypted: u64,
    pub missing: u64,
    pub failed: u64,
    pub dropped: u64,
}

impl RuleStatistics {
    fn add_events(&mut self, event: Event, count: u64) {
        let counter = match event {
            Event::Signed => &mut self.signed,
            Event::Verified => &mut self.verified,
            Event::Accepted => &mut self.accepted,
            Event::Encrypted => &mut self.encrypted,
            Event::Decrypted => &mut self.decrypted,
            Event::Missing => &mut self.missing,
            Event::Failed => &mut self.failed,
            Event::Dropped => &mut self.dropped,
        };
        *counter += count;
    }

    fn add(&mut self, other: &RuleStatistics) {
        self.signed += other.signed;
        self.verified += other.verified;
        self.accepted += other.accepted;
        self.encrypted += other.encrypted;
        self.decrypted += other.decrypted;
        self.missing += other.missing;
        self.failed += other.failed;
        self.dropped += other.dropped;
    }
}

/// Statistics of all security rules, by rule name
#[derive(Debug, Clone, Default)]
pub struct Statistics {
    rules: HashMap<String, RuleStatistics>,
}

impl Statistics {
    pub fn record(&mut self, rule: &str, event: Event) {
        self.record_n(rule, event, 1);
    }

    /// Records an event several times, e.g., one per security block.
    pub fn record_n(&mut self, rule: &str, event: Event, count: u64) {
        self.rules
            .entry(rule.to_string())
            .or_default()
            .add_events(event, count);
    }

    pub fn rule(&self, rule: &str) -> RuleStatistics {
        self.rules.get(rule).copied().unwrap_or_default()
    }

    /// Adds the counters of other statistics, e.g., those of a single bundle.
    pub fn merge(&mut self, other: &Statistics) {
        for (rule, statistics) in &other.rules {
            self.rules.entry(rule.clone()).or_default().add(statistics);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_counted_per_rule() {
        let mut bundle = Statistics::default();
        bundle.record("sign", Event::Signed);
        bundle.record("accept", Event::Failed);
        bundle.record("accept", Event::Dropped);

        let mut total = Statistics::default();
        total.merge(&bundle);
        total.merge(&bundle);
        assert_eq!(total.rule("sign").signed, 2);
        assert_eq!(
            total.rule("accept"),
            RuleStatistics {
                failed: 2,
                dropped: 2,
                ..Default::default()
            }
        );
        assert_eq!(total.rule("unknown"), RuleStatistics::default());
    }
}
