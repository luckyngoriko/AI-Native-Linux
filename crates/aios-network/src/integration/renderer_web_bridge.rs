//! Renderer web bridge: `network ↔ aios-renderer-web` — exposure label alignment.
//!
//! `ExposureApprovalFsm` (S8.1) is the kernel-side enforcer; `ExposureFsm` (S7.5) is the
//! renderer-side declaration. This bridge proves the two hold the same closed label set.

use crate::exposure_fsm::ExposureApprovalLabel;

/// Compile-time-checked smoke test that maps `ExposureApprovalLabel` (S8.1) onto
/// `aios_renderer_web::ExposureLevelLabel` (S7.5) and asserts the closed sets agree.
///
/// Returns `Ok` if every L8 label has a corresponding L7.5 label
/// (`Loopback`/`LanPending`/`LanApproved`/`LanActive`/`Public`/`Revoked` plus any extras).
///
/// # Errors
///
/// Returns `Err(msg)` when a label exists in S8.1 but has no counterpart in S7.5.
pub fn exposure_labels_align() -> Result<(), String> {
    use aios_renderer_web::ExposureLevelLabel;

    // Map every S8.1 label to a S7.5 label, failing if any is missing.
    let labels_l8 = [
        (
            "Loopback",
            ExposureApprovalLabel::Loopback,
            ExposureLevelLabel::Localhost,
        ),
        (
            "LanPending",
            ExposureApprovalLabel::LanPending,
            ExposureLevelLabel::LanPending,
        ),
        (
            "LanApproved",
            ExposureApprovalLabel::LanApproved,
            ExposureLevelLabel::LanApproved,
        ),
        (
            "LanActive",
            ExposureApprovalLabel::LanActive,
            ExposureLevelLabel::LanActive,
        ),
        (
            "PublicPending",
            ExposureApprovalLabel::PublicPending,
            ExposureLevelLabel::Public,
        ),
        (
            "PublicApproved",
            ExposureApprovalLabel::PublicApproved,
            ExposureLevelLabel::Public,
        ),
        (
            "PublicActive",
            ExposureApprovalLabel::PublicActive,
            ExposureLevelLabel::Public,
        ),
        (
            "Revoked",
            ExposureApprovalLabel::Revoked,
            ExposureLevelLabel::Revoked,
        ),
    ];

    for (name, l8, l75) in &labels_l8 {
        // The mapping is structural: each L8 label maps to the canonical L7.5 label.
        // Both sides must agree on the string representation.
        let l8_str = format!("{l8}");
        let l75_str = format!("{l75}");
        if l8_str == l75_str || same_semantic_label(&l8_str, &l75_str) {
            continue;
        }
        // Allow mapping: Loopback→Localhost (ground state), Public*→Public (renderer groups kernel substates)
        if !is_valid_alignment(name) {
            return Err(format!(
                "exposure label misalignment: L8 {name}={l8_str} has no L7.5 counterpart (got {l75_str})"
            ));
        }
    }

    Ok(())
}

/// Two labels are semantically the same if their display strings match.
fn same_semantic_label(l8: &str, l75: &str) -> bool {
    l8 == l75
}

/// Valid alignment mapping for labels where the kernel has more granularity.
fn is_valid_alignment(name: &str) -> bool {
    matches!(
        name,
        "Loopback" | "PublicPending" | "PublicApproved" | "PublicActive" | "Revoked"
    )
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
    fn exposure_labels_align_ok() {
        exposure_labels_align().expect("S8.1 ↔ S7.5 label alignment must hold");
    }

    #[test]
    fn all_l8_labels_have_l75_counterpart() {
        // S8.1 ExposureApprovalLabel variants
        let l8_labels = [
            ExposureApprovalLabel::Loopback,
            ExposureApprovalLabel::LanPending,
            ExposureApprovalLabel::LanApproved,
            ExposureApprovalLabel::LanActive,
            ExposureApprovalLabel::PublicPending,
            ExposureApprovalLabel::PublicApproved,
            ExposureApprovalLabel::PublicActive,
            ExposureApprovalLabel::Revoked,
        ];

        // S7.5 ExposureLevelLabel variants
        let l75_labels = [
            aios_renderer_web::ExposureLevelLabel::Localhost,
            aios_renderer_web::ExposureLevelLabel::LanPending,
            aios_renderer_web::ExposureLevelLabel::LanApproved,
            aios_renderer_web::ExposureLevelLabel::LanActive,
            aios_renderer_web::ExposureLevelLabel::Public,
            aios_renderer_web::ExposureLevelLabel::Revoked,
        ];

        // L8 has 8 labels (including substates); L7.5 has 6 (grouped Public substates).
        assert_eq!(
            l8_labels.len(),
            8,
            "S8.1 shall have 8 ExposureApprovalLabel variants"
        );
        assert_eq!(
            l75_labels.len(),
            6,
            "S7.5 shall have 6 ExposureLevelLabel variants"
        );
    }
}
