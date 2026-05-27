//! T-136 — compile-time guard: the crate builds without `qt-bridge` feature.
//! The real validation is that `cargo check --workspace` succeeds with the
//! default feature set (i.e. qt-bridge OFF).

#![cfg(not(feature = "qt-bridge"))]

/// Trivially passes regardless of feature state.
/// The actual value of this test is that it compiles — proving the crate
/// is valid without Qt6 dependencies.
#[test]
fn qt_bridge_compiles_feature_off() {
    let has_qt = cfg!(feature = "qt-bridge");
    let no_qt = !cfg!(feature = "qt-bridge");
    // Always true: at least one of these must hold
    assert!(has_qt || no_qt);
}
