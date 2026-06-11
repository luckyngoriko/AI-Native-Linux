//! `aios-fleet` — typed core skeleton for S25 Fleet, Cluster, and Remote Execution.
//!
//! Provides the type surface for cluster trust roots, fleet membership state machines,
//! federated identity across realms, cross-org trust delegation, and remote workload
//! routing decisions.

#![forbid(unsafe_code)]

pub mod cluster_root;
pub mod enums;
pub mod federated_identity;
pub mod membership;
pub mod remote_routing;
pub mod trust_delegation;

pub use cluster_root::ClusterTrustRoot;
pub use enums::{
    ClusterOverlayMode, ClusterRole, ClusterTrustScope, FleetMembershipState,
    RemoteRoutingClass, RemoteRoutingReason, TrustDelegationDirection,
};
pub use federated_identity::{FederatedIdentityBundle, FederatedSubjectId};
pub use membership::FleetMembership;
pub use remote_routing::RemoteWorkloadRouting;
pub use trust_delegation::CrossOrgTrustDelegation;
