//! M19 closure invariants — `aios-distribution` v0.1.0 (S11.1).
//!
//! Closure-criteria checks (MILESTONES §"Closure criteria" #5/#6): the crate
//! version marker is correct and the headline closed vocabularies are complete.
//! `todo!()`/`unimplemented!()`/`unsafe`/`unwrap` are forbidden in `src/` by the
//! workspace lints, so this file asserts the version + surface-completeness markers.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use aios_distribution::{
    extended_60m_variants, forever_variants, standard_24m_variants, DistributionRecordType,
};

/// #5 — M19 closes `aios-distribution` at version `0.1.0` (MILESTONES closure
/// criterion #5: every milestone bumps its crate `0.0.1` → `0.1.0`). Closing M19
/// marks **Rev.2 FULL-REAL** — 19/19 implementation milestones.
#[test]
fn version_marker_is_0_1_0() {
    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        "0.1.0",
        "M19 closes aios-distribution at v0.1.0"
    );
}

/// #6 — the distribution evidence vocabulary is complete (19 record types, fully
/// partitioned into retention classes).
#[test]
fn evidence_vocabulary_complete_and_partitioned() {
    assert_eq!(DistributionRecordType::all().len(), 19);
    assert_eq!(
        forever_variants().len() + extended_60m_variants().len() + standard_24m_variants().len(),
        19
    );
}
