//! Takedown-reason vocabulary per S11.1 §3.9.
//!
//! Every `PublisherDeplatformed` event records exactly one `TakedownReason` in
//! its `FOREVER` evidence record.  The reason governs the grace-period duration,
//! rotation discipline, and re-admission path.

use serde::{Deserialize, Serialize};

/// Closed enum — 7 reasons per S11.1 §3.9.
///
/// | Variant                      | S11.1 label                    | Semantics                                                                        |
/// |------------------------------|--------------------------------|----------------------------------------------------------------------------------|
/// | `MaliciousBehaviorDetected`  | `MALICIOUS_BEHAVIOR_DETECTED`  | Runtime breach, capability abuse, or malicious behaviour confirmed by review.    |
/// | `SupplyChainCompromise`      | `SUPPLY_CHAIN_COMPROMISE`      | Publisher's build pipeline or signing infrastructure was compromised.            |
/// | `CapabilityLieDetected`      | `CAPABILITY_LIE_DETECTED`      | First-run capability audit detected sustained drift across packages.             |
/// | `LegalRequirement`           | `LEGAL_REQUIREMENT`            | Mandatory takedown under operator-jurisdiction legal order.                      |
/// | `PublisherRequest`           | `PUBLISHER_REQUEST`            | Publisher voluntarily withdrew.                                                  |
/// | `KeyCompromise`              | `KEY_COMPROMISE`               | Publisher root key or package signing key confirmed compromised.                 |
/// | `AbandonedAfterInactiveTtl`  | `ABANDONED_AFTER_INACTIVE_TTL` | Publisher inactive beyond configurable TTL (default 24 months).                  |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakedownReason {
    /// Runtime breach, capability abuse, or operator-flagged malicious behaviour.
    MaliciousBehaviorDetected,
    /// Publisher's build pipeline or signing infrastructure was compromised.
    SupplyChainCompromise,
    /// First-run capability audit detected sustained drift.
    CapabilityLieDetected,
    /// Mandatory takedown under operator-jurisdiction legal requirement.
    LegalRequirement,
    /// Publisher voluntarily withdrew (e.g. organisational dissolution, key retirement).
    PublisherRequest,
    /// Publisher root key (or package signing key) confirmed compromised.
    KeyCompromise,
    /// Publisher inactive beyond configurable TTL (default 24 months).
    AbandonedAfterInactiveTtl,
}
