use crate::enums::TimeTrustGrade;

#[derive(Debug, Clone)]
pub struct SkewBudget {
    pub budget_id: String,
    pub profile: String,
    pub soft_skew_ms: i64,
    pub hard_skew_ms: i64,
    pub max_unsynced_seconds: i64,
    pub backward_jump_tolerance_ms: i64,
    pub require_grade_floor: TimeTrustGrade,
}

impl SkewBudget {
    pub fn new(
        profile: &str,
        soft_skew_ms: i64,
        hard_skew_ms: i64,
        max_unsynced_seconds: i64,
        backward_jump_tolerance_ms: i64,
        require_grade_floor: TimeTrustGrade,
    ) -> Self {
        Self {
            budget_id: format!("skewbudget_{}", ulid::Ulid::new()),
            profile: profile.to_string(),
            soft_skew_ms,
            hard_skew_ms,
            max_unsynced_seconds,
            backward_jump_tolerance_ms,
            require_grade_floor,
        }
    }

    pub fn for_profile(profile: &str) -> Self {
        match profile {
            "DEV_RELAXED" => Self::new(
                "DEV_RELAXED",
                i64::MAX,
                i64::MAX,
                i64::MAX,
                i64::MAX,
                TimeTrustGrade::UntrustedLocal,
            ),
            "SECURE_DEFAULT" => Self::new(
                "SECURE_DEFAULT",
                1000,
                5000,
                3600,
                5000,
                TimeTrustGrade::AttestedSingle,
            ),
            "STIG_ALIGNED" => Self::new(
                "STIG_ALIGNED",
                1000,
                2000,
                900,
                2000,
                TimeTrustGrade::AttestedSingle,
            ),
            "AIRGAP_HIGH" => Self::new(
                "AIRGAP_HIGH",
                1000,
                2000,
                600,
                2000,
                TimeTrustGrade::AttestedSingle,
            ),
            _ => Self::new(
                profile,
                1000,
                5000,
                3600,
                5000,
                TimeTrustGrade::AttestedSingle,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_relaxed_has_max_tolerances() {
        let budget = SkewBudget::for_profile("DEV_RELAXED");
        assert_eq!(budget.soft_skew_ms, i64::MAX);
        assert_eq!(budget.hard_skew_ms, i64::MAX);
        assert_eq!(budget.max_unsynced_seconds, i64::MAX);
        assert_eq!(budget.backward_jump_tolerance_ms, i64::MAX);
        assert_eq!(budget.require_grade_floor, TimeTrustGrade::UntrustedLocal);
    }

    #[test]
    fn secure_default_tightens_skew() {
        let budget = SkewBudget::for_profile("SECURE_DEFAULT");
        assert_eq!(budget.soft_skew_ms, 1000);
        assert_eq!(budget.hard_skew_ms, 5000);
        assert_eq!(budget.max_unsynced_seconds, 3600);
        assert_eq!(budget.require_grade_floor, TimeTrustGrade::AttestedSingle);
    }

    #[test]
    fn stig_aligned_is_tighter_than_secure_default() {
        let stig = SkewBudget::for_profile("STIG_ALIGNED");
        let secure = SkewBudget::for_profile("SECURE_DEFAULT");
        assert!(stig.hard_skew_ms < secure.hard_skew_ms);
        assert!(stig.max_unsynced_seconds < secure.max_unsynced_seconds);
    }

    #[test]
    fn airgap_high_is_most_restrictive() {
        let airgap = SkewBudget::for_profile("AIRGAP_HIGH");
        let stig = SkewBudget::for_profile("STIG_ALIGNED");
        assert!(airgap.max_unsynced_seconds < stig.max_unsynced_seconds);
        assert_eq!(airgap.hard_skew_ms, stig.hard_skew_ms);
    }
}
