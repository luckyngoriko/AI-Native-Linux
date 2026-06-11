use std::fmt;

pub struct FederatedSubjectId {
    pub home_realm: String,
    pub local_id: String,
}

impl fmt::Display for FederatedSubjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.home_realm, self.local_id)
    }
}

pub struct FederatedIdentityBundle {
    pub bundle_id: String,
    pub home_realm: String,
    pub cluster_id: String,
    pub realm_root_pubkey: String,
    pub subjects: Vec<(FederatedSubjectId, bool)>,
}

impl FederatedIdentityBundle {
    #[must_use]
    pub fn new(
        bundle_id: String,
        home_realm: String,
        cluster_id: String,
        realm_root_pubkey: String,
        subjects: Vec<(FederatedSubjectId, bool)>,
    ) -> Self {
        Self {
            bundle_id,
            home_realm,
            cluster_id,
            realm_root_pubkey,
            subjects,
        }
    }
}

impl FederatedSubjectId {
    #[must_use]
    pub fn resolve_legacy(legacy_id: &str) -> Self {
        Self {
            home_realm: "realm:default".into(),
            local_id: legacy_id.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_format() {
        let sid = FederatedSubjectId {
            home_realm: "realm:aios-alpha".into(),
            local_id: "subject-42".into(),
        };
        assert_eq!(sid.to_string(), "realm:aios-alpha:subject-42");
    }

    #[test]
    fn resolve_legacy_wraps_in_realm_default() {
        let sid = FederatedSubjectId::resolve_legacy("user-99");
        assert_eq!(sid.home_realm, "realm:default");
        assert_eq!(sid.local_id, "user-99");
        assert_eq!(sid.to_string(), "realm:default:user-99");
    }

    #[test]
    fn bundle_new_stores_subjects() {
        let sid = FederatedSubjectId {
            home_realm: "realm:x".into(),
            local_id: "s1".into(),
        };
        let bundle = FederatedIdentityBundle::new(
            "bndl_01".into(),
            "realm:x".into(),
            "clr_01".into(),
            "pk_hex".into(),
            vec![(sid, true)],
        );
        assert_eq!(bundle.bundle_id, "bndl_01");
        assert_eq!(bundle.subjects.len(), 1);
        assert!(bundle.subjects[0].1);
    }
}
