//! Takedown-reason vocabulary per S11.1 §3.10.
//!
//! Every `PublisherDeplatformed` event records exactly one `TakedownReason` in
//! its `FOREVER` evidence record.  The reason governs the grace-period duration,
//! rotation discipline, and re-admission path.

use serde::{Deserialize, Serialize};

/// Closed enum — 7 reasons per S11.1 §3.10.
///
/// | Variant                      | S11.1 label                    |
/// |------------------------------|--------------------------------|
/// | `Malware`                    | `MALICIOUS_BEHAVIOR_DETECTED`  |
/// | `SupplyChainCompromise`      | `SUPPLY_CHAIN_COMPROMISE`      |
/// | `CapabilityLieDetected`      | `CAPABILITY_LIE_DETECTED`      |
/// | `LicenseViolation`           | `LEGAL_REQUIREMENT`            |
/// | `KeyCompromise`              | `KEY_COMPROMISE`               |
/// | `AbandonedAfterInactiveTtl`  | `ABANDONED_AFTER_INACTIVE_TTL` |
/// | `OperatorRequested`          | `PUBLISHER_REQUEST`            |
///
/// Deviation: spec §3.9 uses `MALICIOUS_BEHAVIOR_DETECTED`,
/// `LEGAL_REQUIREMENT`, `PUBLISHER_REQUEST`.  T-187 uses task-authorised names
/// (`Malware`, `LicenseViolation`, `OperatorRequested`) that compress the
/// semantic into single-word Rust variants while preserving the underlying
/// takedown categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakedownReason {
    /// Runtime breach, capability abuse, or operator-flagged malicious behaviour.
    Malware,
    /// Publisher's build pipeline or signing infrastructure was compromised.
    SupplyChainCompromise,
    /// First-run capability audit detected sustained drift.
    CapabilityLieDetected,
    /// Mandatory takedown under operator-jurisdiction legal requirement.
    LicenseViolation,
    /// Publisher root key (or package signing key) confirmed compromised.
    KeyCompromise,
    /// Publisher inactive beyond configurable TTL (default 24 months).
    AbandonedAfterInactiveTtl,
    /// Operator-requested takedown (voluntary withdrawal or administrative action).
    OperatorRequested,
}
