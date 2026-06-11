pub struct ClusterTrustRoot {
    pub cluster_id: String,
    pub cluster_root_pubkey: String,
    pub rotation_index: u64,
    pub bound_realm: String,
}

impl ClusterTrustRoot {
    #[must_use]
    pub fn new(
        cluster_id: String,
        cluster_root_pubkey: String,
        rotation_index: u64,
        bound_realm: String,
    ) -> Self {
        Self {
            cluster_id,
            cluster_root_pubkey,
            rotation_index,
            bound_realm,
        }
    }

    #[must_use]
    pub fn is_rotated(&self, other: &Self) -> bool {
        other.rotation_index > self.rotation_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_root(index: u64) -> ClusterTrustRoot {
        ClusterTrustRoot::new(
            "clr_01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            "ed25519_pubkey_hex".into(),
            index,
            "realm:aios-default".into(),
        )
    }

    #[test]
    fn rotation_index_increasing() {
        let old = mk_root(1);
        let new = mk_root(2);
        assert!(old.is_rotated(&new));
    }

    #[test]
    fn rotation_index_same_is_not_rotated() {
        let old = mk_root(3);
        let same = mk_root(3);
        assert!(!old.is_rotated(&same));
    }

    #[test]
    fn rotation_index_decreasing_is_not_rotated() {
        let old = mk_root(5);
        let older = mk_root(3);
        assert!(!old.is_rotated(&older));
    }
}
