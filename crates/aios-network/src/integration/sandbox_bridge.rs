//! Sandbox bridge: `network ↔ aios-sandbox` — `NetworkMode` floor intersection per INV I6.
//!
//! INV I6 (most-restrictive-wins): a sandbox profile's network posture intersects with the
//! subject's `OutboundDirective`. The more restrictive bound always wins.

use crate::outbound::OutboundDirective;

/// Intersect an `OutboundDirective` with a sandbox `NetworkPosture` floor.
///
/// INV I6 — most-restrictive-wins:
/// - Sandbox `DenyAll` → returns `DenyAll` regardless of directive.
/// - Sandbox `LoopbackOnly` AND directive any → returns `AllowLoopbackOnly`.
/// - Sandbox `HostLimited` or `ExplicitAllowlist` AND directive `AllowInternet` →
///   returns `AllowListOnly { allowlist_id: "<sandbox-floor>" }`.
/// - Sandbox `Full` AND directive any → returns directive unchanged.
#[must_use]
pub fn intersect_with_sandbox_floor(
    directive: OutboundDirective,
    sandbox_posture: aios_sandbox::NetworkPosture,
) -> OutboundDirective {
    match sandbox_posture {
        aios_sandbox::NetworkPosture::DenyAll => OutboundDirective::DenyAll,
        aios_sandbox::NetworkPosture::LoopbackOnly => OutboundDirective::AllowLoopbackOnly,
        aios_sandbox::NetworkPosture::HostLimited
        | aios_sandbox::NetworkPosture::ExplicitAllowlist => match directive {
            OutboundDirective::AllowInternet => OutboundDirective::AllowListOnly {
                allowlist_id: "sandbox-floor".into(),
            },
            other => other,
        },
        aios_sandbox::NetworkPosture::Full => directive,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test"
)]
mod tests {
    use super::*;

    #[test]
    fn deny_all_wins_over_internet() {
        let result = intersect_with_sandbox_floor(
            OutboundDirective::AllowInternet,
            aios_sandbox::NetworkPosture::DenyAll,
        );
        assert_eq!(result, OutboundDirective::DenyAll);
    }

    #[test]
    fn loopback_wins_over_internet() {
        let result = intersect_with_sandbox_floor(
            OutboundDirective::AllowInternet,
            aios_sandbox::NetworkPosture::LoopbackOnly,
        );
        assert_eq!(result, OutboundDirective::AllowLoopbackOnly);
    }

    #[test]
    fn host_limited_downgrades_internet_to_allowlist() {
        let result = intersect_with_sandbox_floor(
            OutboundDirective::AllowInternet,
            aios_sandbox::NetworkPosture::HostLimited,
        );
        assert!(matches!(result, OutboundDirective::AllowListOnly { .. }));
    }

    #[test]
    fn explicit_allowlist_downgrades_internet() {
        let result = intersect_with_sandbox_floor(
            OutboundDirective::AllowInternet,
            aios_sandbox::NetworkPosture::ExplicitAllowlist,
        );
        assert!(matches!(result, OutboundDirective::AllowListOnly { .. }));
    }

    #[test]
    fn full_preserves_original_directive() {
        let result = intersect_with_sandbox_floor(
            OutboundDirective::AllowInternet,
            aios_sandbox::NetworkPosture::Full,
        );
        assert_eq!(result, OutboundDirective::AllowInternet);
    }

    #[test]
    fn full_preserves_denyall() {
        let result = intersect_with_sandbox_floor(
            OutboundDirective::DenyAll,
            aios_sandbox::NetworkPosture::Full,
        );
        assert_eq!(result, OutboundDirective::DenyAll);
    }
}
