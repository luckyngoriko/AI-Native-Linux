use crate::enums::TimeTrustGrade;

#[derive(Debug, Clone)]
pub struct TimeGradeBinding {
    pub grade: TimeTrustGrade,
    pub bound_at: chrono::DateTime<chrono::Utc>,
    pub immutable: bool,
}

impl TimeGradeBinding {
    pub fn bind(grade: TimeTrustGrade) -> Self {
        Self {
            grade,
            bound_at: chrono::Utc::now(),
            immutable: true,
        }
    }

    pub fn try_upgrade(&mut self, _new_grade: TimeTrustGrade) -> Result<(), String> {
        Err("time trust grade cannot be retroactively upgraded (INV-034)".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_binding_is_immutable() {
        let binding = TimeGradeBinding::bind(TimeTrustGrade::AttestedSingle);
        assert!(binding.immutable);
    }

    #[test]
    fn upgrade_is_rejected() {
        let mut binding = TimeGradeBinding::bind(TimeTrustGrade::AttestedSingle);
        let result = binding.try_upgrade(TimeTrustGrade::AttestedQuorum);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("INV-034"));
    }

    #[test]
    fn grade_unchanged_after_failed_upgrade() {
        let mut binding = TimeGradeBinding::bind(TimeTrustGrade::AttestedSingle);
        let _ = binding.try_upgrade(TimeTrustGrade::AttestedQuorum);
        assert_eq!(binding.grade, TimeTrustGrade::AttestedSingle);
        assert!(binding.immutable);
    }

    #[test]
    fn binding_timestamp_is_set() {
        let before = chrono::Utc::now();
        let binding = TimeGradeBinding::bind(TimeTrustGrade::UntrustedLocal);
        let after = chrono::Utc::now();
        assert!(binding.bound_at >= before);
        assert!(binding.bound_at <= after);
    }
}
