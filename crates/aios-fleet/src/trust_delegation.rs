use crate::enums::TrustDelegationDirection;

pub struct CrossOrgTrustDelegation {
    pub delegation_id: String,
    pub from_realm: String,
    pub to_realm: String,
    pub direction: TrustDelegationDirection,
    pub forbid_ai_subjects: bool,
    pub forbid_admin_actions: bool,
}

impl CrossOrgTrustDelegation {
    #[must_use]
    pub fn new(
        delegation_id: String,
        from_realm: String,
        to_realm: String,
        direction: TrustDelegationDirection,
    ) -> Self {
        Self {
            delegation_id,
            from_realm,
            to_realm,
            direction,
            forbid_ai_subjects: true,
            forbid_admin_actions: true,
        }
    }

    #[must_use]
    pub fn is_delegation_safe(&self) -> bool {
        self.forbid_ai_subjects && self.forbid_admin_actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_delegation() -> CrossOrgTrustDelegation {
        CrossOrgTrustDelegation::new(
            "del_01".into(),
            "realm:a".into(),
            "realm:b".into(),
            TrustDelegationDirection::Bidirectional,
        )
    }

    #[test]
    fn new_delegation_forbids_ai_subjects() {
        let d = mk_delegation();
        assert!(d.forbid_ai_subjects);
    }

    #[test]
    fn new_delegation_forbids_admin_actions() {
        let d = mk_delegation();
        assert!(d.forbid_admin_actions);
    }

    #[test]
    fn delegation_is_safe_by_default() {
        let d = mk_delegation();
        assert!(d.is_delegation_safe());
    }

    #[test]
    fn delegation_unsafe_if_ai_not_forbidden() {
        let mut d = mk_delegation();
        d.forbid_ai_subjects = false;
        assert!(!d.is_delegation_safe());
    }

    #[test]
    fn delegation_unsafe_if_admin_not_forbidden() {
        let mut d = mk_delegation();
        d.forbid_admin_actions = false;
        assert!(!d.is_delegation_safe());
    }
}
