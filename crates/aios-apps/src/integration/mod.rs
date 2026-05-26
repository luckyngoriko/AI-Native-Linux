//! Cross-crate integration shims — apps owns the seam.
//!
//! Each bridge module translates aios-apps lifecycle operations into calls
//! on the upstream crate traits without modifying any upstream source file.

pub mod runtime_bridge;
pub mod sandbox_bridge;
pub mod sgr_bridge;

pub use runtime_bridge::RuntimeBridge;
pub use sandbox_bridge::SandboxBridge;
pub use sgr_bridge::SgrBridge;
