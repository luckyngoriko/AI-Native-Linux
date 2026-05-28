use serde::{Deserialize, Serialize};

/// Canonical subject identifier (e.g., `"human:lucky"`, `"agent:planner-7"`, `"service:aios-apps"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubjectId(pub String);

impl std::fmt::Display for SubjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Per-group identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupId(pub String);

impl std::fmt::Display for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    #[test]
    fn subject_id_serde_round_trip() {
        let id = SubjectId("human:lucky".into());
        let json = serde_json::to_string(&id).unwrap();
        let back: SubjectId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn group_id_serde_round_trip() {
        let id = GroupId("group:operators".into());
        let json = serde_json::to_string(&id).unwrap();
        let back: GroupId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
