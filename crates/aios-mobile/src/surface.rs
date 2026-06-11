//! Mobile surface profile — represents a registered mobile device with
//! attestation and capabilities.

use crate::enums::{MobileFormFactor, MobileSurfaceMode, MobileTransport};

/// A registered mobile surface capable of receiving and approving typed
/// action requests from the AIOS host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MobileSurface {
    /// Unique surface identifier (format `msrf_<ULID>`).
    pub surface_id: String,
    /// Rendering mode of the surface.
    pub mode: MobileSurfaceMode,
    /// Physical form factor of the device.
    pub form_factor: MobileFormFactor,
    /// Transport mechanism for host communication.
    pub transport: MobileTransport,
    /// Whether the device has a hardware-backed keystore.
    pub device_attestation: bool,
    /// Whether the surface can act as an approval endpoint.
    pub can_approve: bool,
    /// Whether the surface can issue emergency stop signals.
    pub can_emergency_stop: bool,
    /// Whether the surface can quarantine installed applications.
    pub can_quarantine_app: bool,
    /// Lifecycle state — always `"Registered"` on creation.
    pub lifecycle_state: String,
}

impl MobileSurface {
    /// Creates a new `MobileSurface` with a freshly generated surface ID.
    #[must_use]
    pub fn new(
        mode: MobileSurfaceMode,
        form_factor: MobileFormFactor,
        transport: MobileTransport,
        device_attestation: bool,
        can_approve: bool,
        can_emergency_stop: bool,
        can_quarantine_app: bool,
    ) -> Self {
        let surface_id = format!("msrf_{}", ulid::Ulid::new());
        Self {
            surface_id,
            mode,
            form_factor,
            transport,
            device_attestation,
            can_approve,
            can_emergency_stop,
            can_quarantine_app,
            lifecycle_state: "Registered".to_string(),
        }
    }

    /// Returns `true` if this surface is capable of approval decisions.
    #[must_use]
    pub fn is_approval_capable(&self) -> bool {
        self.can_approve
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn constructor_creates_registered_surface() {
        let surface = MobileSurface::new(
            MobileSurfaceMode::AiOSMobileRenderer,
            MobileFormFactor::Phone,
            MobileTransport::LanDirect,
            true,
            true,
            false,
            false,
        );
        assert!(surface.surface_id.starts_with("msrf_"));
        assert_eq!(surface.lifecycle_state, "Registered");
        assert!(surface.is_approval_capable());
    }

    #[test]
    fn approval_capability_reflects_flag() {
        let approving = MobileSurface::new(
            MobileSurfaceMode::AiOSMobileRenderer,
            MobileFormFactor::Tablet,
            MobileTransport::RelayAuthenticated,
            true,
            true,
            true,
            false,
        );
        assert!(approving.is_approval_capable());
        assert!(approving.can_emergency_stop);

        let non_approving = MobileSurface::new(
            MobileSurfaceMode::AiOSPhoneEdition,
            MobileFormFactor::WatchGlance,
            MobileTransport::OfflineToken,
            false,
            false,
            false,
            false,
        );
        assert!(!non_approving.is_approval_capable());
    }

    #[test]
    fn surface_ids_are_unique() {
        let s1 = MobileSurface::new(
            MobileSurfaceMode::AiOSMobileRenderer,
            MobileFormFactor::Phone,
            MobileTransport::LanDirect,
            true,
            true,
            false,
            false,
        );
        let s2 = MobileSurface::new(
            MobileSurfaceMode::AiOSMobileRenderer,
            MobileFormFactor::Phone,
            MobileTransport::LanDirect,
            true,
            true,
            false,
            false,
        );
        assert_ne!(s1.surface_id, s2.surface_id);
    }
}
