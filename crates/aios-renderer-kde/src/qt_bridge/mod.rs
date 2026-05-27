//! T-136 — cxx-qt bridge module (S7.4 §4).
//!
//! Gated behind `#[cfg(feature = "qt-bridge")]` — headless CI skips Qt6 build.
//! This module allows `unsafe_code` because cxx-qt procedural macros generate
//! `unsafe` FFI blocks; the workspace default of `deny(unsafe_code)` is relaxed
//! only here.

#![cfg(feature = "qt-bridge")]
#![allow(unsafe_code)]

pub mod aios_window;

// Re-export QObject bindings so callers can access them via `qt_bridge::AiosWindow`.
pub use aios_window::qobject::AiosApprovalPrompt;
pub use aios_window::qobject::AiosWindow;
