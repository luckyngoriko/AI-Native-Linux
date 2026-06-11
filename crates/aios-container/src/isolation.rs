use crate::enums::IsolationLevel;

/// Selects the appropriate isolation level based on trust posture and ABI
/// requirements.
pub struct SecureRuntimeSelector;

impl SecureRuntimeSelector {
    /// Select the isolation level for a given trust posture.
    ///
    /// Trust levels:
    /// - `"trusted"` with signature → `Rootless`
    /// - `"unknown"` or unsigned → `GVisor`
    /// - `"untrusted"` with Linux ABI requirement → `Kata`
    /// - Fallback → `FullVm`
    /// - Portable plugin → `Wasm`
    pub fn select(trust_level: &str, needs_linux_abi: bool) -> IsolationLevel {
        match trust_level {
            "trusted" => IsolationLevel::Rootless,
            "unknown" => IsolationLevel::GVisor,
            "untrusted" if needs_linux_abi => IsolationLevel::Kata,
            "untrusted" => IsolationLevel::FullVm,
            "plugin" => IsolationLevel::Wasm,
            _ => IsolationLevel::FullVm,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    #[test]
    fn trusted_gets_rootless() {
        assert_eq!(
            SecureRuntimeSelector::select("trusted", false),
            IsolationLevel::Rootless
        );
        assert_eq!(
            SecureRuntimeSelector::select("trusted", true),
            IsolationLevel::Rootless
        );
    }

    #[test]
    fn unknown_unsigned_gets_gvisor() {
        assert_eq!(
            SecureRuntimeSelector::select("unknown", false),
            IsolationLevel::GVisor
        );
        assert_eq!(
            SecureRuntimeSelector::select("unknown", true),
            IsolationLevel::GVisor
        );
    }

    #[test]
    fn untrusted_linux_gets_kata() {
        assert_eq!(
            SecureRuntimeSelector::select("untrusted", true),
            IsolationLevel::Kata
        );
    }

    #[test]
    fn untrusted_non_linux_gets_fullvm() {
        assert_eq!(
            SecureRuntimeSelector::select("untrusted", false),
            IsolationLevel::FullVm
        );
    }

    #[test]
    fn plugin_gets_wasm() {
        assert_eq!(
            SecureRuntimeSelector::select("plugin", false),
            IsolationLevel::Wasm
        );
    }

    #[test]
    fn unrecognized_falls_back_to_fullvm() {
        assert_eq!(
            SecureRuntimeSelector::select("some_random_string", true),
            IsolationLevel::FullVm
        );
    }
}
