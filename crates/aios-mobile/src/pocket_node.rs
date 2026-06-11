//! Pocket node — a mobile device that holds one shard of a vault split
//! and acts as a low-power replica for evidence and emergency recovery.

use serde::{Deserialize, Serialize};

/// Roles a pocket node may fulfill in the AIOS distributed architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PocketNodeRole {
    /// Holds one shard of a threshold vault split.
    VaultShardHolder,
    /// Replicates evidence log segments for durability.
    EvidenceReplica,
    /// Holds an emergency recovery key shard.
    EmergencyRecoveryKey,
    /// Runs a low-power local AI inference endpoint.
    LowPowerAiLocal,
}

/// A pocket node — a mobile device registered as a shard holder in the
/// AIOS vault threshold scheme. A single shard alone cannot reconstruct
/// the vault; at least the threshold number of shards is required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PocketNode {
    /// Unique node identifier (format `pnode_<ULID>`).
    pub node_id: String,
    /// The surface this pocket node is associated with.
    pub surface_id: String,
    /// Roles assigned to this pocket node.
    pub roles: Vec<PocketNodeRole>,
    /// Index of this node's shard within the threshold scheme (if shard holder).
    pub vault_shard_index: Option<u32>,
    /// Total number of shards in the threshold scheme (if shard holder).
    pub vault_shard_total: Option<u32>,
}

impl PocketNode {
    /// Creates a new pocket node with the given surface and roles.
    #[must_use]
    pub fn new(
        surface_id: String,
        roles: Vec<PocketNodeRole>,
        vault_shard_index: Option<u32>,
        vault_shard_total: Option<u32>,
    ) -> Self {
        let node_id = format!("pnode_{}", ulid::Ulid::new());
        Self {
            node_id,
            surface_id,
            roles,
            vault_shard_index,
            vault_shard_total,
        }
    }

    /// Returns `false` — a single pocket node shard can never reconstruct
    /// the vault on its own. Reconstruction requires at least the threshold
    /// number of shards.
    #[must_use]
    pub fn can_reconstruct(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn constructor_creates_node_with_id_prefix() {
        let node = PocketNode::new(
            "msrf_01TEST".to_string(),
            vec![PocketNodeRole::VaultShardHolder],
            Some(0),
            Some(3),
        );
        assert!(node.node_id.starts_with("pnode_"));
        assert_eq!(node.surface_id, "msrf_01TEST");
        assert_eq!(node.vault_shard_index, Some(0));
        assert_eq!(node.vault_shard_total, Some(3));
    }

    #[test]
    fn single_shard_cannot_reconstruct() {
        let node = PocketNode::new(
            "msrf_01TEST".to_string(),
            vec![PocketNodeRole::VaultShardHolder],
            Some(0),
            Some(3),
        );
        assert!(!node.can_reconstruct());
    }

    #[test]
    fn non_shard_node_cannot_reconstruct() {
        let node = PocketNode::new(
            "msrf_02TEST".to_string(),
            vec![PocketNodeRole::LowPowerAiLocal],
            None,
            None,
        );
        assert!(!node.can_reconstruct());
    }

    #[test]
    fn multiple_roles_can_be_assigned() {
        let node = PocketNode::new(
            "msrf_03TEST".to_string(),
            vec![
                PocketNodeRole::VaultShardHolder,
                PocketNodeRole::EvidenceReplica,
                PocketNodeRole::EmergencyRecoveryKey,
            ],
            Some(1),
            Some(5),
        );
        assert_eq!(node.roles.len(), 3);
        assert!(!node.can_reconstruct());
    }
}
