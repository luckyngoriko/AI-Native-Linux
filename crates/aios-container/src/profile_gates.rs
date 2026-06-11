/// Returns `true` if privileged containers are allowed without human approval
/// under the given security profile.
///
/// Only `"DEV_RELAXED"` allows privileged execution without additional gates.
pub fn is_privileged_allowed(profile: &str) -> bool {
    matches!(profile, "DEV_RELAXED")
}

/// Returns `true` if unsigned container images are allowed under the given
/// security profile.
///
/// `"STIG_ALIGNED"` and `"AIRGAP_HIGH"` forbid unsigned images.
pub fn is_unsigned_allowed(profile: &str) -> bool {
    !matches!(profile, "STIG_ALIGNED" | "AIRGAP_HIGH")
}

/// Returns `true` if the profile requires digest-pinned image references.
///
/// `"STIG_ALIGNED"` and `"AIRGAP_HIGH"` require digest pins.
pub fn requires_digest_pin(profile: &str) -> bool {
    matches!(profile, "STIG_ALIGNED" | "AIRGAP_HIGH")
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    #[test]
    fn privileged_only_allowed_in_dev_relaxed() {
        assert!(is_privileged_allowed("DEV_RELAXED"));
        assert!(!is_privileged_allowed("STIG_ALIGNED"));
        assert!(!is_privileged_allowed("AIRGAP_HIGH"));
        assert!(!is_privileged_allowed("UNKNOWN"));
    }

    #[test]
    fn unsigned_forbidden_in_strict_profiles() {
        assert!(is_unsigned_allowed("DEV_RELAXED"));
        assert!(!is_unsigned_allowed("STIG_ALIGNED"));
        assert!(!is_unsigned_allowed("AIRGAP_HIGH"));
        assert!(is_unsigned_allowed("UNKNOWN"));
    }

    #[test]
    fn digest_pin_required_in_strict_profiles() {
        assert!(!requires_digest_pin("DEV_RELAXED"));
        assert!(requires_digest_pin("STIG_ALIGNED"));
        assert!(requires_digest_pin("AIRGAP_HIGH"));
        assert!(!requires_digest_pin("UNKNOWN"));
    }
}
