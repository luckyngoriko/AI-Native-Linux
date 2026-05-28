//! Firewall rule model (S8.1 §10).
//!
//! nftables-compatible rule types + iptables fallback marker as pure-Rust types.
//! No actual `nft` / `iptables` subprocess calls — these model the rule pipeline
//! that later milestones wire to the kernel backend.  The iptables fallback path
//! sets a FOREVER evidence flag (evidence emission deferred to T-161).

use std::net::IpAddr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};
use tokio::sync::RwLock;

use crate::error::NetworkPolicyError;
use crate::evidence::{NetworkEvidenceEmitter, WithEmitter};
use crate::ids::SubjectId;
use crate::outbound::OutboundDirective;
use crate::protocol::ProtocolFamily;

// ---------------------------------------------------------------------------
// FirewallBackend — closed 2-variant (S8.1 §10)
// ---------------------------------------------------------------------------

/// Closed firewall backend vocabulary (S8.1 §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FirewallBackend {
    /// Primary — nftables kernel backend.
    Nftables,
    /// Fallback — iptables legacy backend (FOREVER evidence emitted separately).
    IptablesFallback,
}

// ---------------------------------------------------------------------------
// FirewallChain — closed 5-variant (S8.1 §10)
// ---------------------------------------------------------------------------

/// Closed netfilter chain vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FirewallChain {
    /// Inbound chain.
    Input,
    /// Outbound chain.
    Output,
    /// Forward chain.
    Forward,
    /// Pre-routing chain.
    Prerouting,
    /// Post-routing chain.
    Postrouting,
}

// ---------------------------------------------------------------------------
// FirewallAction — closed 5-variant (S8.1 §10)
// ---------------------------------------------------------------------------

/// Closed firewall verdict vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FirewallAction {
    /// Accept the packet.
    Accept,
    /// Silently drop the packet.
    Drop,
    /// Reject with ICMP / RST.
    Reject,
    /// Log and continue to next rule.
    Log,
    /// Return to calling chain.
    Return,
}

// ---------------------------------------------------------------------------
// FirewallMatch — closed match vocabulary (S8.1 §10)
// ---------------------------------------------------------------------------

/// Closed firewall match expression vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirewallMatch {
    /// Match a source IP address.
    SourceIp(IpAddr),
    /// Match a source CIDR block.
    SourceCidr(String),
    /// Match a destination IP address.
    DestIp(IpAddr),
    /// Match a destination CIDR block.
    DestCidr(String),
    /// Match a destination port with protocol.
    DestPort {
        /// Port number.
        port: u16,
        /// Protocol family.
        protocol: ProtocolFamily,
    },
    /// Match a network interface by name.
    Interface(String),
    /// Match conntrack state (e.g. `"established,related"`).
    CtState(String),
    /// Match everything — used with [`FirewallAction::Log`] for audit lines.
    All,
}

// ---------------------------------------------------------------------------
// FirewallRule
// ---------------------------------------------------------------------------

/// A single firewall rule (S8.1 §10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirewallRule {
    /// Unique rule identifier.
    pub rule_id: String,
    /// Which chain the rule belongs to.
    pub chain: FirewallChain,
    /// Priority within the chain (lower = evaluated first).
    pub priority: i32,
    /// Match expression.
    #[serde(rename = "match")]
    pub match_expr: FirewallMatch,
    /// Verdict when the match expression succeeds.
    pub action: FirewallAction,
    /// Human-readable comment.
    pub comment: String,
}

// ---------------------------------------------------------------------------
// FirewallRuleset
// ---------------------------------------------------------------------------

/// A complete firewall ruleset (S8.1 §10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirewallRuleset {
    /// Backend this ruleset targets.
    pub backend: FirewallBackend,
    /// Ordered list of rules.
    pub rules: Vec<FirewallRule>,
    /// Monotonic generation counter.
    pub generation: u64,
    /// When this ruleset was built.
    pub built_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// FirewallRulesetBuilder
// ---------------------------------------------------------------------------

/// Pure-Rust composer for [`FirewallRuleset`].
///
/// # Example
///
/// ```ignore
/// let ruleset = FirewallRulesetBuilder::new(FirewallBackend::Nftables)
///     .rule(FirewallRule { ... })
///     .rule(FirewallRule { ... })
///     .build();
/// ```
pub struct FirewallRulesetBuilder {
    backend: FirewallBackend,
    rules: Vec<FirewallRule>,
}

impl FirewallRulesetBuilder {
    /// Create a new builder targeting the given backend.
    #[must_use]
    pub const fn new(backend: FirewallBackend) -> Self {
        Self {
            backend,
            rules: Vec::new(),
        }
    }

    /// Append a rule and return the builder for chaining.
    #[must_use]
    pub fn rule(mut self, rule: FirewallRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Produce a [`FirewallRuleset`] with `built_at = Utc::now()` and
    /// `generation` set to the current epoch-millisecond timestamp.
    #[must_use]
    pub fn build(self) -> FirewallRuleset {
        let now = Utc::now();
        let generation = now.timestamp_millis().unsigned_abs();
        FirewallRuleset {
            backend: self.backend,
            rules: self.rules,
            generation,
            built_at: now,
        }
    }
}

// ---------------------------------------------------------------------------
// FirewallManager
// ---------------------------------------------------------------------------

/// Central firewall rule lifecycle manager (S8.1 §10).
///
/// Stores the active ruleset, a history of prior rulesets, and a
/// fallback-active flag that flips to `true` when the backend degrades to
/// `IptablesFallback` (FOREVER evidence emission deferred to T-161).
pub struct FirewallManager {
    /// Currently active ruleset.
    active: RwLock<Option<FirewallRuleset>>,
    /// Prior rulesets (most recent last).
    history: RwLock<Vec<FirewallRuleset>>,
    /// Whether the system is currently running on the iptables fallback path.
    fallback_active: RwLock<bool>,
    /// Optional evidence emitter.
    emitter: RwLock<Option<Arc<dyn NetworkEvidenceEmitter>>>,
}

impl FirewallManager {
    /// Create an empty firewall manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: RwLock::new(None),
            history: RwLock::new(Vec::new()),
            fallback_active: RwLock::new(false),
            emitter: RwLock::new(None),
        }
    }

    /// Apply a new ruleset, pushing the current active one to history.
    ///
    /// If the ruleset backend is [`FirewallBackend::IptablesFallback`], flips
    /// `fallback_active` to `true`.
    ///
    /// # Errors
    ///
    /// Never fails for T-159 — always returns `Ok(())`.
    pub async fn apply_ruleset(&self, ruleset: FirewallRuleset) -> Result<(), NetworkPolicyError> {
        let is_fallback = ruleset.backend == FirewallBackend::IptablesFallback;

        let mut active = self.active.write().await;
        if let Some(prior) = active.take() {
            self.history.write().await.push(prior);
        }
        *active = Some(ruleset);
        drop(active);

        if is_fallback {
            *self.fallback_active.write().await = true;
            if let Some(ref e) = *self.emitter.read().await {
                let _ = e
                    .emit_firewall_fallback_activated(FirewallBackend::IptablesFallback)
                    .await;
            }
        }
        Ok(())
    }

    /// Return a clone of the currently active ruleset, if any.
    pub async fn active_ruleset(&self) -> Option<FirewallRuleset> {
        self.active.read().await.clone()
    }

    /// Return the full ruleset history (most recent last).
    pub async fn history(&self) -> Vec<FirewallRuleset> {
        self.history.read().await.clone()
    }

    /// Whether the system is in iptables fallback mode.
    pub async fn is_in_fallback(&self) -> bool {
        *self.fallback_active.read().await
    }

    /// Compile an [`OutboundDirective`] into a deterministic list of
    /// [`FirewallRule`]s.
    ///
    /// | Directive          | Output                                          |
    /// |--------------------|--------------------------------------------------|
    /// | `DenyAll`           | 1 rule: `Output` + source IP placeholder + `Drop` |
    /// | `AllowLoopbackOnly`  | 2 rules: allow loopback, drop all else            |
    /// | all other variants  | empty list (deferred to later milestone)          |
    #[must_use]
    #[allow(
        clippy::unused_async,
        reason = "async retained per S8.1 §10 spec; wired in T-161"
    )]
    pub async fn enforce_subject_directive(
        &self,
        _subject: &SubjectId,
        directive: &OutboundDirective,
    ) -> Vec<FirewallRule> {
        match directive {
            OutboundDirective::DenyAll => {
                vec![FirewallRule {
                    rule_id: "deny-all-default".into(),
                    chain: FirewallChain::Output,
                    priority: i32::MAX,
                    match_expr: FirewallMatch::SourceIp(IpAddr::V4(
                        std::net::Ipv4Addr::UNSPECIFIED,
                    )),
                    action: FirewallAction::Drop,
                    comment: "default deny-all directive for subject".into(),
                }]
            }
            OutboundDirective::AllowLoopbackOnly => {
                vec![
                    FirewallRule {
                        rule_id: "allow-loopback".into(),
                        chain: FirewallChain::Output,
                        priority: 100,
                        match_expr: FirewallMatch::DestCidr("127.0.0.0/8".into()),
                        action: FirewallAction::Accept,
                        comment: "allow loopback".into(),
                    },
                    FirewallRule {
                        rule_id: "deny-all-else".into(),
                        chain: FirewallChain::Output,
                        priority: i32::MAX,
                        match_expr: FirewallMatch::All,
                        action: FirewallAction::Drop,
                        comment: "drop all non-loopback".into(),
                    },
                ]
            }
            // Other variants — empty stub; wired in a later milestone.
            OutboundDirective::AllowListOnly { .. }
            | OutboundDirective::AllowVpnOnly { .. }
            | OutboundDirective::AllowInternet => {
                vec![]
            }
        }
    }
}

impl WithEmitter for FirewallManager {
    fn with_emitter(mut self, emitter: Option<Arc<dyn NetworkEvidenceEmitter>>) -> Self {
        self.emitter = RwLock::new(emitter);
        self
    }
}

impl Default for FirewallManager {
    #[allow(clippy::missing_const_for_fn)]
    fn default() -> Self {
        Self::new()
    }
}
