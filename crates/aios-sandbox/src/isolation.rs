use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed isolation-kind vocabulary for sandbox profiles (S3.2).
///
/// Determines the kernel-level isolation boundary. Adding a variant is a
/// versioned spec change.
///
/// `NO_ISOLATION` exists for enumeration completeness but is forbidden by
/// policy in production — the runtime safety floor rejects it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IsolationKind {
    /// Linux namespace isolation (user, mount, net, pid, uts, ipc).
    NamespaceLocal,
    /// Full process-container isolation (OCI runtime — Podman/Docker adapter).
    ProcessContainer,
    /// Hardware-virtualised guest (KVM/QEMU).
    VmGuest,
    /// Browser origin-based isolation (per-origin iframe / `GPUAdapter` sandbox).
    BrowserOriginIsolated,
    /// No isolation — direct host execution. Forbidden by runtime safety floor.
    NoIsolation,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use strum::{EnumCount, IntoEnumIterator};

    #[test]
    fn isolation_kind_has_five_variants() {
        assert_eq!(IsolationKind::COUNT, 5);
        assert_eq!(IsolationKind::iter().count(), 5);
    }

    #[test]
    fn isolation_kind_serde_round_trip() {
        for variant in IsolationKind::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: IsolationKind = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
    }
}
