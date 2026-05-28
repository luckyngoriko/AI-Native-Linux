//! L8 Network Policy for AIOS (S8.1).
//!
//! Typed core skeleton: closed vocabulary + error enum + identifier types.
//! Trait, controllers, grants, FSM, evaluator, AI discipline, DNS/VPN,
//! firewall, gRPC, evidence, cross-crate land in later tasks.

#![forbid(unsafe_code)]

/// AI cross-origin posture (INV I4).
pub mod ai_cross_origin;
/// Allowlist entry types.
pub mod allowlist;
/// Network policy controller trait and in-memory implementation.
pub mod controller;
/// Network policy error taxonomy.
pub mod error;
/// Subject and group identifier newtypes.
pub mod ids;
/// Inbound exposure + port policy.
pub mod inbound;
/// Outbound directive vocabulary.
pub mod outbound;
/// System-wide network posture.
pub mod posture;
/// Protocol family vocabulary.
pub mod protocol;

pub use ai_cross_origin::AICrossOriginPosture;
pub use allowlist::{AllowlistEntry, AllowlistEntryKind};
pub use controller::{
    ConnectionDecision, EvaluateConnectionRequest, InMemoryNetworkPolicyController,
    NetworkPolicyController, PostureChangeReceipt,
};
pub use error::{NetworkPolicyError, NetworkPolicyErrorCode};
pub use ids::{GroupId, SubjectId};
pub use inbound::{InboundExposureClass, PortPolicy};
pub use outbound::OutboundDirective;
pub use posture::NetworkPosture;
pub use protocol::ProtocolFamily;

/// Crate version marker used by closure-invariant tests in T-162.
pub const DEFAULT_CODE_VERSION: &str = "aios-network/0.0.1-T151";
