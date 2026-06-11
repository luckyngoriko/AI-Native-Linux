#![forbid(unsafe_code)]

pub mod attestation;
pub mod consequential_gate;
pub mod enums;
pub mod grade_binding;
pub mod posture;
pub mod skew_budget;
pub mod skew_detector;

pub use attestation::TimeAttestation;
pub use consequential_gate::is_consequential_action_allowed;
pub use enums::{SkewClassification, TimePostureState, TimeTrustGrade, TrustedTimeSource};
pub use grade_binding::TimeGradeBinding;
pub use posture::TimePosture;
pub use skew_budget::SkewBudget;
pub use skew_detector::ClockSkewDetector;
